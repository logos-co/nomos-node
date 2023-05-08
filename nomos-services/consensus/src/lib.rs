//! In this module, and children ones, the 'view lifetime is tied to a logical consensus view,
//! represented by the `View` struct.
//! This is done to ensure that all the different data structs used to represent various actors
//! are always synchronized (i.e. it cannot happen that we accidentally use committees from different views).
//! It's obviously extremely important that the information contained in `View` is synchronized across different
//! nodes, but that has to be achieved through different means.
mod network;
mod tally;
mod tip;

// std
use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::time::Duration;
// crates
use futures::{future::FusedFuture, stream::FuturesUnordered, FutureExt, Stream, StreamExt};
use serde::{de::DeserializeOwned, Serialize};
// internal
use crate::network::messages::{NewViewMsg, ProposalChunkMsg, TimeoutMsg, TimeoutQcMsg, VoteMsg};
use crate::network::NetworkAdapter;
use crate::tally::CarnotTally;
use consensus_engine::{
    AggregateQc, Carnot, Committee, NewView, Overlay, Payload, Qc, StandardQc, Timeout, TimeoutQc,
    Vote,
};
use nomos_core::block::Block;
use nomos_core::crypto::PublicKey;
use nomos_core::fountain::FountainCode;
use nomos_core::tx::Transaction;
use nomos_core::vote::Tally;
use nomos_mempool::{
    backend::MemPool, network::NetworkAdapter as MempoolAdapter, MempoolMsg, MempoolService,
};
use nomos_network::NetworkService;
use overwatch_rs::services::relay::{OutboundRelay, Relay};
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::NoMessage,
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};
use tip::Tip;

// Raw bytes for now, could be a ed25519 public key
pub type NodeId = PublicKey;
// Random seed for each round provided by the protocol
pub type Seed = [u8; 32];

#[derive(Debug)]
pub struct CarnotSettings<Fountain: FountainCode, VoteTally: Tally> {
    private_key: [u8; 32],
    fountain_settings: Fountain::Settings,
    tally_settings: VoteTally::Settings,
}

impl<Fountain: FountainCode, VoteTally: Tally> Clone for CarnotSettings<Fountain, VoteTally> {
    fn clone(&self) -> Self {
        Self {
            private_key: self.private_key,
            fountain_settings: self.fountain_settings.clone(),
            tally_settings: self.tally_settings.clone(),
        }
    }
}

impl<Fountain: FountainCode, VoteTally: Tally> CarnotSettings<Fountain, VoteTally> {
    #[inline]
    pub const fn new(
        private_key: [u8; 32],
        fountain_settings: Fountain::Settings,
        tally_settings: VoteTally::Settings,
    ) -> Self {
        Self {
            private_key,
            fountain_settings,
            tally_settings,
        }
    }
}

pub struct CarnotConsensus<A, P, M, F, O>
where
    F: FountainCode,
    A: NetworkAdapter,
    M: MempoolAdapter<Tx = P::Tx>,
    P: MemPool,
    O: Overlay,
    P::Tx: Transaction + Debug + 'static,
    <P::Tx as Transaction>::Hash: Debug,
    A::Backend: 'static,
{
    service_state: ServiceStateHandle<Self>,
    // underlying networking backend. We need this so we can relay and check the types properly
    // when implementing ServiceCore for CarnotConsensus
    network_relay: Relay<NetworkService<A::Backend>>,
    mempool_relay: Relay<MempoolService<M, P>>,
    _fountain: std::marker::PhantomData<F>,
    _overlay: std::marker::PhantomData<O>,
}

impl<A, P, M, F, O> ServiceData for CarnotConsensus<A, P, M, F, O>
where
    F: FountainCode,
    A: NetworkAdapter,
    P: MemPool,
    P::Tx: Transaction + Debug,
    <P::Tx as Transaction>::Hash: Debug,
    M: MempoolAdapter<Tx = P::Tx>,
    O: Overlay,
{
    const SERVICE_ID: ServiceId = "Carnot";
    type Settings = CarnotSettings<F, CarnotTally>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NoMessage;
}

#[async_trait::async_trait]
impl<A, P, M, F, O> ServiceCore for CarnotConsensus<A, P, M, F, O>
where
    F: FountainCode + Clone + Send + Sync + 'static,
    A: NetworkAdapter + Clone + Send + Sync + 'static,
    P: MemPool + Send + Sync + 'static,
    P::Settings: Send + Sync + 'static,
    P::Tx:
        Debug + Clone + Eq + Hash + Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
    <P::Tx as Transaction>::Hash: Debug + Send + Sync,
    M: MempoolAdapter<Tx = P::Tx> + Send + Sync + 'static,
    O: Overlay + Send + Sync + 'static,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        let network_relay = service_state.overwatch_handle.relay();
        let mempool_relay = service_state.overwatch_handle.relay();
        Ok(Self {
            service_state,
            network_relay,
            _fountain: Default::default(),
            _overlay: Default::default(),
            mempool_relay,
        })
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
        let network_relay: OutboundRelay<_> = self
            .network_relay
            .connect()
            .await
            .expect("Relay connection with NetworkService should succeed");

        let mempool_relay: OutboundRelay<_> = self
            .mempool_relay
            .connect()
            .await
            .expect("Relay connection with MemPoolService should succeed");

        let CarnotSettings {
            private_key,
            fountain_settings,
            tally_settings,
        } = self.service_state.settings_reader.get_updated_settings();

        let network_adapter = A::new(network_relay).await;

        let tip = Tip;

        let fountain = F::new(fountain_settings);
        let tally = CarnotTally::new(tally_settings);
        let overlay = O::new();

        let genesis = consensus_engine::Block {
            id: [0; 32],
            view: 0,
            parent_qc: Qc::Standard(StandardQc::genesis()),
        };
        let mut carnot = Carnot::from_genesis(private_key, genesis, overlay);

        loop {
            // we iterate view per view because it's easier to keep an ordering for events
            carnot = View {
                timeout: Duration::from_secs(2),
                committee: Committee::new(),
                adapter: network_adapter.clone(),
                tally: tally.clone(),
                carnot,
                fountain: fountain.clone(),
                tip: tip.clone(),
                mempool: mempool_relay.clone(),
            }
            .run()
            .await;
        }
    }
}

#[allow(dead_code)] // TODO: remove this when using broadcasting events
enum Output<Tx: Clone + Eq + Hash> {
    Send(consensus_engine::Send),
    BroadcastTimeoutQc { timeout_qc: TimeoutQc },
    BroadcastProposal { proposal: Block<Tx> },
}

pub struct View<A, F, O: Overlay, Tx: Transaction> {
    timeout: Duration,
    committee: Committee,
    adapter: A,
    fountain: F,
    tally: CarnotTally,
    carnot: Carnot<O>,
    tip: Tip,
    mempool: OutboundRelay<MempoolMsg<Tx>>,
}

impl<A, O, F, Tx> View<A, F, O, Tx>
where
    A: NetworkAdapter,
    F: FountainCode,
    O: Overlay + Clone,
    Tx: Transaction + Clone + Eq + Hash + Serialize + DeserializeOwned + Debug + Send + Sync,
{
    fn view(&self) -> consensus_engine::View {
        self.carnot.current_view()
    }

    pub async fn proposal_stream(&self) -> impl Stream<Item = Block<Tx>> {
        self.adapter
            .proposal_chunks_stream(self.view())
            .await
            .filter_map(move |msg| {
                async move {
                    let proposal = Block::from_bytes(&msg.chunk);
                    if proposal.header().id == msg.proposal {
                        // TODO: Leader is faulty? what should we do?
                        Some(proposal)
                    } else {
                        None
                    }
                }
            })
    }

    async fn timeout_qc_stream(&self) -> impl Stream<Item = TimeoutQc> {
        self.adapter
            .timeout_qc_stream(self.view())
            .await
            .map(|msg| msg.qc)
    }

    async fn gather_votes(
        &self,
        committee: &Committee,
        block: consensus_engine::Block,
        tally: &CarnotTally,
    ) -> (Qc, HashSet<Vote>, consensus_engine::Block) {
        let votes_stream = self
            .adapter
            .votes_stream(committee, block.view, block.id)
            .await;
        match tally.tally(block.view, votes_stream).await {
            Ok((qc, outcome)) => (qc, outcome, block),
            Err(_e) => {
                todo!("Handle tally error");
            }
        }
    }

    async fn gather_new_views(
        &self,
        committee: &Committee,
        view: consensus_engine::View,
    ) -> (HashSet<NewView>, TimeoutQc) {
        match self.carnot.last_view_timeout_qc() {
            // only try to collect new views if we have a timeout qc from the previous view
            Some(timeout_qc) if timeout_qc.view == view - 1 => {
                // TODO: Maybe implement tally for unhappy path?
                let mut seen = HashSet::new();
                let mut votes = HashSet::new();
                let mut stream = self
                    .adapter
                    .new_view_stream(committee, timeout_qc.view + 1)
                    .await;
                while let Some(msg) = StreamExt::next(&mut stream).await {
                    if seen.contains(&msg.voter) {
                        continue;
                    }
                    seen.insert(msg.voter);
                    votes.insert(msg.vote);
                    // TODO: when building this for the leader we should use overlay.leader_supermajority_threshold
                    if votes.len() >= self.carnot.super_majority_threshold() {
                        break;
                    }
                }
                (votes, timeout_qc)
            }
            _ => {
                futures::pending!();
                unreachable!()
            }
        }
    }

    async fn gather_timeout(&self) -> HashSet<Timeout> {
        self.adapter
            .timeout_stream(&self.committee, self.view())
            .await
            .take(self.carnot.leader_super_majority_threshold())
            .map(|msg| msg.vote)
            .collect()
            .await
    }

    async fn get_previous_block_qc(&self) -> Qc {
        // if we're not the leader we don't have to do anything
        if !self.carnot.is_leader_for_current_view() {
            futures::pending!();
        }

        let previous_view = self.carnot.current_view() - 1;
        let leader_committee = [self.carnot.id()].into_iter().collect();
        let mut happy_path = self
            .carnot
            .blocks_in_view(previous_view)
            .into_iter()
            .map(|block| async {
                // TODO: this should use overlay.leader_super_majority_threshold
                self.gather_votes(&leader_committee, block, &self.tally)
                    .await
            })
            .collect::<FuturesUnordered<_>>();

        tokio::select! {
            Some((qc, _, _)) = happy_path.next() => {
                qc
            }
            _votes = self.gather_new_views(&leader_committee, previous_view) => {
                Qc::Aggregated(AggregateQc {
                    high_qc: self.carnot.high_qc(),
                    view: self.carnot.current_view(),
                })
            }
        }
    }

    pub async fn run(self) -> Carnot<O> {
        let mut carnot = self.carnot.clone();
        // Some tasks are only activated after another event has been triggered,
        // We thus push them to this stream and wait for them on demand
        let mut gather_block_votes = FuturesUnordered::new();

        // we create futures here and just poll them through a mut reference in the loop
        // to avoid creating a new future for each iteration
        let proposal_stream = self.proposal_stream().await;
        let mut timeout_qc_stream = self.timeout_qc_stream().await;
        let local_timeout = tokio::time::sleep(self.timeout);
        let root_timeout = self.gather_timeout().fuse();
        let get_previous_block_qc = self.get_previous_block_qc().fuse();
        let gather_new_view_votes = self.gather_new_views(&self.committee, self.view()).fuse();

        tokio::pin!(proposal_stream);
        tokio::pin!(local_timeout);
        tokio::pin!(root_timeout);
        tokio::pin!(get_previous_block_qc);
        tokio::pin!(gather_new_view_votes);

        loop {
            let mut output = None;
            tokio::select! {
                Some(block) = proposal_stream.next() => {
                    if let Ok(new_state) = carnot.receive_block(block.header().clone()) {
                        carnot = new_state;
                        gather_block_votes.push(self.gather_votes(&self.committee, block.header().clone(), &self.tally));
                    }
                }

                Some((_qc, _votes, block)) = gather_block_votes.next() => {
                    let (new_carnot, out) = carnot.approve_block(block);
                    carnot = new_carnot;
                    output = Some(Output::Send::<Tx>(out));
                }

                Some(timeout_qc) = timeout_qc_stream.next()  => {
                    carnot = carnot.receive_timeout_qc(timeout_qc.clone());
                }

                (new_views, timeout_qc) = &mut gather_new_view_votes, if !gather_new_view_votes.is_terminated() => {
                    let (new_carnot, out) =
                    carnot.approve_new_view(timeout_qc, new_views);
                    carnot = new_carnot;
                    output = Some(Output::Send(out));
                }

                _ = &mut local_timeout => {
                    let (new_carnot, out) =
                    carnot.local_timeout();
                    carnot = new_carnot;
                    output = out.map(Output::Send);
                }

                _ = &mut root_timeout, if !root_timeout.is_terminated() => {
                    // timeout detected
                }

                qc = &mut get_previous_block_qc, if !get_previous_block_qc.is_terminated() => {
                    // propose block
                    let (reply_channel, rx) = tokio::sync::oneshot::channel();
                    self.mempool.send(MempoolMsg::View { ancestor_hint: self.tip.id(), reply_channel })
                        .await
                        .unwrap_or_else(|(e, _)| eprintln!("Could not get transactions from mempool {e}"));
                    if let Ok(txs) = rx.await {
                        let proposal = Block::new(self.view(), qc, txs);
                        output = Some(Output::BroadcastProposal { proposal });
                    }
                }
            }
            if let Some(output) = output {
                handle_output(
                    &self.adapter,
                    &self.fountain,
                    carnot.current_view(),
                    carnot.id(),
                    output,
                )
                .await;
            }
            // Break after we've
            if carnot.current_view() != self.view() {
                break;
            }
        }
        carnot
    }
}

async fn handle_output<A, F, Tx>(
    adapter: &A,
    fountain: &F,
    view: i64,
    node_id: NodeId,
    output: Output<Tx>,
) where
    A: NetworkAdapter,
    F: FountainCode,
    Tx: Hash + Eq + Clone + Serialize + DeserializeOwned,
{
    match output {
        Output::Send(consensus_engine::Send { to, payload }) => match payload {
            Payload::Vote(vote) => {
                adapter
                    .send(
                        &to,
                        view,
                        VoteMsg {
                            voter: node_id,
                            vote,
                            qc: None, // TODO: handle root commmittee members
                        }
                        .as_bytes(),
                        "votes",
                    )
                    .await;
            }
            Payload::Timeout(timeout) => {
                adapter
                    .send(
                        &to,
                        view,
                        TimeoutMsg {
                            voter: node_id,
                            vote: timeout,
                        }
                        .as_bytes(),
                        "timeout",
                    )
                    .await;
            }
            Payload::NewView(new_view) => {
                adapter
                    .send(
                        &to,
                        view,
                        NewViewMsg {
                            voter: node_id,
                            vote: new_view,
                        }
                        .as_bytes(),
                        "new-view",
                    )
                    .await;
            }
        },
        Output::BroadcastProposal { proposal } => {
            fountain
                .encode(&proposal.as_bytes())
                .for_each(|chunk| {
                    adapter.broadcast_block_chunk(ProposalChunkMsg {
                        proposal: proposal.header().id,
                        chunk: chunk.to_vec().into_boxed_slice(),
                        view,
                    })
                })
                .await;
        }
        Output::BroadcastTimeoutQc { timeout_qc } => {
            adapter
                .broadcast_timeout_qc(TimeoutQcMsg {
                    source: node_id,
                    qc: timeout_qc,
                })
                .await;
        }
    }
}
