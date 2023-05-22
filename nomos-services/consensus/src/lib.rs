//! In this module, and children ones, the 'view lifetime is tied to a logical consensus view,
//! represented by the `View` struct.
//! This is done to ensure that all the different data structs used to represent various actors
//! are always synchronized (i.e. it cannot happen that we accidentally use committees from different views).
//! It's obviously extremely important that the information contained in `View` is synchronized across different
//! nodes, but that has to be achieved through different means.
pub mod network;
pub mod overlay;
mod tally;
mod view_cancel;

// std
use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::pin::Pin;
use std::time::Duration;
// crates
use futures::{stream::FuturesUnordered, Future, Stream, StreamExt};
use serde::Deserialize;
use serde::{de::DeserializeOwned, Serialize};
// internal
use crate::network::messages::{NewViewMsg, ProposalChunkMsg, TimeoutMsg, TimeoutQcMsg, VoteMsg};
use crate::network::NetworkAdapter;
use crate::tally::{happy::CarnotTally, unhappy::NewViewTally, CarnotTallySettings};
use crate::view_cancel::ViewCancelCache;
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

// TODO: tale this from config
const TIMEOUT: Duration = Duration::from_secs(60);

// Raw bytes for now, could be a ed25519 public key
pub type NodeId = PublicKey;
// Random seed for each round provided by the protocol
pub type Seed = [u8; 32];

#[derive(Debug, Deserialize, Serialize)]
pub struct CarnotSettings<Fountain: FountainCode> {
    private_key: [u8; 32],
    fountain_settings: Fountain::Settings,
    nodes: Vec<NodeId>,
}

impl<Fountain: FountainCode> Clone for CarnotSettings<Fountain> {
    fn clone(&self) -> Self {
        Self {
            private_key: self.private_key,
            fountain_settings: self.fountain_settings.clone(),
            nodes: self.nodes.clone(),
        }
    }
}

impl<Fountain: FountainCode> CarnotSettings<Fountain> {
    #[inline]
    pub const fn new(
        private_key: [u8; 32],
        fountain_settings: Fountain::Settings,
        nodes: Vec<NodeId>,
    ) -> Self {
        Self {
            private_key,
            fountain_settings,
            nodes,
        }
    }
}

pub struct CarnotConsensus<A, P, M, F, O>
where
    F: FountainCode,
    A: NetworkAdapter,
    M: MempoolAdapter<Tx = P::Tx>,
    P: MemPool,
    O: Overlay + Debug,
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
    O: Overlay + Debug,
{
    const SERVICE_ID: ServiceId = "Carnot";
    type Settings = CarnotSettings<F>;
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
    O: Overlay + Debug + Send + Sync + 'static,
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
            nodes,
        } = self.service_state.settings_reader.get_updated_settings();

        let overlay = O::new(nodes);
        let genesis = consensus_engine::Block {
            id: [0; 32],
            view: 0,
            parent_qc: Qc::Standard(StandardQc::genesis()),
        };
        let mut carnot = Carnot::from_genesis(private_key, genesis, overlay);
        let network_adapter = A::new(network_relay).await;
        let adapter = &network_adapter;
        let _self_committee = carnot.self_committee();
        let self_committee = &_self_committee;
        let _leader_committee = [carnot.id()].into_iter().collect();
        let leader_committee = &_leader_committee;
        let fountain = F::new(fountain_settings);
        let _tally_settings = CarnotTallySettings {
            threshold: carnot.super_majority_threshold(),
            participating_nodes: carnot.child_committees().into_iter().flatten().collect(),
        };
        let tally_settings = &_tally_settings;
        let _leader_tally_settings = CarnotTallySettings {
            threshold: carnot.leader_super_majority_threshold(),
            // TODO: add children of root committee
            participating_nodes: carnot.root_committee(),
        };
        let leader_tally_settings = &_leader_tally_settings;

        let mut view_cancel_cache = ViewCancelCache::new();

        let events: FuturesUnordered<Pin<Box<dyn Future<Output = Event<P::Tx>> + Send>>> =
            FuturesUnordered::new();
        let genesis_block = carnot.genesis_block();
        events.push(Box::pin(view_cancel_cache.cancelable_event_future(
            genesis_block.view + 1,
            Self::gather_block(adapter, genesis_block.view + 1),
        )));
        events.push(Box::pin(view_cancel_cache.cancelable_event_future(
            genesis_block.view,
            Self::gather_votes(
                adapter,
                self_committee,
                genesis_block.clone(),
                tally_settings.clone(),
            ),
        )));
        if carnot.is_leader_for_view(genesis_block.view + 1) {
            events.push(Box::pin(view_cancel_cache.cancelable_event_future(
                genesis_block.view + 1,
                async move {
                    let Event::Approve { qc, .. } = Self::gather_votes(
                    adapter,
                    leader_committee,
                    genesis_block,
                    leader_tally_settings.clone(),
                )
                .await else { unreachable!() };
                    Event::ProposeBlock { qc }
                },
            )));
        }

        tokio::pin!(events);

        while let Some(event) = events.next().await {
            let mut output = None;
            let prev_view = carnot.current_view();
            match event {
                Event::Proposal { block, mut stream } => {
                    tracing::debug!("received proposal {:?}", block);
                    let block = block.header().clone();
                    match carnot.receive_block(block.clone()) {
                        Ok(new_state) => {
                            let new_view = new_state.current_view();
                            if new_view != carnot.current_view() {
                                events.push(Box::pin(view_cancel_cache.cancelable_event_future(
                                    block.view,
                                    Self::gather_votes(
                                        adapter,
                                        self_committee,
                                        block.clone(),
                                        tally_settings.clone(),
                                    ),
                                )));
                            } else {
                                events.push(Box::pin(view_cancel_cache.cancelable_event_future(
                                    block.view,
                                    async move {
                                        if let Some(block) = stream.next().await {
                                            Event::Proposal { block, stream }
                                        } else {
                                            Event::None
                                        }
                                    },
                                )));
                            }
                            carnot = new_state;
                        }
                        Err(_) => tracing::debug!("invalid block {:?}", block),
                    }
                    if carnot.is_leader_for_view(block.view + 1) {
                        events.push(Box::pin(view_cancel_cache.cancelable_event_future(
                            block.view,
                            async move {
                                let Event::Approve { qc, .. } = Self::gather_votes(
                                adapter,
                                leader_committee,
                                block,
                                leader_tally_settings.clone(),
                            )
                            .await else { unreachable!() };
                                Event::ProposeBlock { qc }
                            },
                        )));
                    }
                }
                Event::Approve { block, .. } => {
                    tracing::debug!("approving proposal {:?}", block);
                    let (new_carnot, out) = carnot.approve_block(block.clone());
                    carnot = new_carnot;
                    output = Some(Output::Send::<P::Tx>(out));
                }
                Event::LocalTimeout => {
                    tracing::debug!("local timeout");
                    let (new_carnot, out) = carnot.local_timeout();
                    carnot = new_carnot;
                    output = out.map(Output::Send);
                }
                Event::NewView {
                    timeout_qc,
                    new_views,
                } => {
                    tracing::debug!("approving new view {:?}", timeout_qc);
                    let (new_carnot, out) = carnot.approve_new_view(timeout_qc.clone(), new_views);
                    carnot = new_carnot;
                    output = Some(Output::Send(out));
                    let next_view = timeout_qc.view + 2;
                    if carnot.is_leader_for_view(next_view) {
                        let high_qc = carnot.high_qc();
                        events.push(Box::pin(view_cancel_cache.cancelable_event_future(
                            timeout_qc.view + 1,
                            async move {
                                let _votes = Self::gather_new_views(
                                    adapter,
                                    leader_committee,
                                    timeout_qc,
                                    leader_tally_settings.clone(),
                                )
                                .await;
                                Event::ProposeBlock {
                                    qc: Qc::Aggregated(AggregateQc {
                                        high_qc,
                                        view: next_view,
                                    }),
                                }
                            },
                        )));
                    }
                }
                Event::TimeoutQc { timeout_qc } => {
                    tracing::debug!("timeout received {:?}", timeout_qc);
                    carnot = carnot.receive_timeout_qc(timeout_qc.clone());
                    events.push(Box::pin(view_cancel_cache.cancelable_event_future(
                        timeout_qc.view + 1,
                        Self::gather_new_views(
                            adapter,
                            self_committee,
                            timeout_qc,
                            tally_settings.clone(),
                        ),
                    )));
                }
                Event::RootTimeout { timeouts } => {
                    tracing::debug!("root timeout {:?}", timeouts);
                    // TODO: filter timeouts upon reception
                    assert!(timeouts.iter().all(|t| t.view == carnot.current_view()));
                    let high_qc = timeouts
                        .iter()
                        .map(|t| &t.high_qc)
                        .chain(std::iter::once(&carnot.high_qc()))
                        .max_by_key(|qc| qc.view)
                        .expect("empty root committee")
                        .clone();
                    if carnot.is_member_of_root_committee() {
                        let timeout_qc = TimeoutQc {
                            view: carnot.current_view(),
                            high_qc,
                            sender: carnot.id(),
                        };
                        output = Some(Output::BroadcastTimeoutQc { timeout_qc });
                    }
                }
                Event::ProposeBlock { qc } => {
                    tracing::debug!("proposing block");
                    let (reply_channel, rx) = tokio::sync::oneshot::channel();
                    mempool_relay
                        .send(MempoolMsg::View {
                            ancestor_hint: [0; 32],
                            reply_channel,
                        })
                        .await
                        .unwrap_or_else(|(e, _)| {
                            eprintln!("Could not get transactions from mempool {e}")
                        });
                    match rx.await {
                        Ok(txs) => {
                            let proposal = Block::new(qc.view() + 1, qc, txs);
                            output = Some(Output::BroadcastProposal { proposal });
                        }
                        Err(e) => tracing::error!("Could not fetch txs {e}"),
                    }
                }
                Event::None => {}
            }

            let current_view = carnot.current_view();
            if current_view != prev_view {
                // First we cancel previous processing view tasks
                view_cancel_cache.cancel(prev_view);
                tracing::debug!("Advanced view from {prev_view} to {current_view}");
                // View change!
                events.push(Box::pin(view_cancel_cache.cancelable_event_future(
                    current_view,
                    async {
                        tokio::time::sleep(TIMEOUT).await;
                        Event::LocalTimeout
                    },
                )));
                events.push(Box::pin(view_cancel_cache.cancelable_event_future(
                    current_view + 1,
                    Self::gather_block(adapter, current_view + 1),
                )));
                events.push(Box::pin(view_cancel_cache.cancelable_event_future(
                    current_view,
                    Self::gather_timeout_qc(adapter, current_view),
                )));
                if carnot.is_member_of_root_committee() {
                    let threshold = carnot.leader_super_majority_threshold();
                    events.push(Box::pin(view_cancel_cache.cancelable_event_future(
                        current_view,
                        Self::gather_timeout(adapter, self_committee, current_view, threshold),
                    )));
                }
            }

            if let Some(output) = output {
                handle_output(adapter, &fountain, carnot.id(), output).await;
            }
        }

        unreachable!("carnot exited");
    }
}

#[allow(dead_code)] // TODO: remove this when using broadcasting events
enum Output<Tx: Clone + Eq + Hash> {
    Send(consensus_engine::Send),
    BroadcastTimeoutQc { timeout_qc: TimeoutQc },
    BroadcastProposal { proposal: Block<Tx> },
}

impl<A, P, M, F, O> CarnotConsensus<A, P, M, F, O>
where
    F: FountainCode + Clone + Send + Sync + 'static,
    A: NetworkAdapter + Clone + Send + Sync + 'static,
    P: MemPool + Send + Sync + 'static,
    P::Settings: Send + Sync + 'static,
    P::Tx:
        Debug + Clone + Eq + Hash + Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
    <P::Tx as Transaction>::Hash: Debug + Send + Sync,
    M: MempoolAdapter<Tx = P::Tx> + Send + Sync + 'static,
    O: Overlay + Debug + Send + Sync + 'static,
{
    async fn gather_timeout_qc(adapter: &A, view: consensus_engine::View) -> Event<P::Tx> {
        if let Some(timeout_qc) = adapter
            .timeout_qc_stream(view)
            .await
            .map(|msg| msg.qc)
            .next()
            .await
        {
            Event::TimeoutQc { timeout_qc }
        } else {
            Event::None
        }
    }

    async fn gather_votes(
        adapter: &A,
        committee: &Committee,
        block: consensus_engine::Block,
        tally: CarnotTallySettings,
    ) -> Event<P::Tx> {
        let tally = CarnotTally::new(tally);
        let votes_stream = adapter.votes_stream(committee, block.view, block.id).await;
        match tally.tally(block.clone(), votes_stream).await {
            Ok((qc, votes)) => Event::Approve { qc, votes, block },
            Err(_e) => {
                todo!("Handle tally error {_e}");
            }
        }
    }

    async fn gather_new_views(
        adapter: &A,
        committee: &Committee,
        timeout_qc: TimeoutQc,
        tally: CarnotTallySettings,
    ) -> Event<P::Tx> {
        let tally = NewViewTally::new(tally);
        let stream = adapter
            .new_view_stream(committee, timeout_qc.view + 1)
            .await;
        match tally.tally(timeout_qc.clone(), stream).await {
            Ok((_qc, new_views)) => Event::NewView {
                timeout_qc,
                new_views,
            },
            Err(_e) => {
                todo!("Handle tally error {_e}");
            }
        }
    }

    async fn gather_timeout(
        adapter: &A,
        committee: &Committee,
        view: consensus_engine::View,
        threshold: usize,
    ) -> Event<P::Tx> {
        futures::pending!();
        let timeouts = adapter
            .timeout_stream(committee, view)
            .await
            .take(threshold)
            .map(|msg| msg.vote)
            .collect()
            .await;
        Event::RootTimeout { timeouts }
    }

    async fn gather_block(adapter: &A, view: consensus_engine::View) -> Event<P::Tx> {
        let stream = adapter
            .proposal_chunks_stream(view)
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
            });
        let mut stream = Box::pin(stream);
        if let Some(block) = stream.next().await {
            Event::Proposal { block, stream }
        } else {
            Event::None
        }
    }
}

async fn handle_output<A, F, Tx>(adapter: &A, fountain: &F, node_id: NodeId, output: Output<Tx>)
where
    A: NetworkAdapter,
    F: FountainCode,
    Tx: Hash + Eq + Clone + Serialize + DeserializeOwned + Debug,
{
    match output {
        Output::Send(consensus_engine::Send { to, payload }) => match payload {
            Payload::Vote(vote) => {
                adapter
                    .send(
                        &to,
                        vote.view,
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
                        timeout.view,
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
                        new_view.view,
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
                        view: proposal.header().view,
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

pub(crate) enum Event<Tx: Clone + Hash + Eq> {
    Proposal {
        block: Block<Tx>,
        stream: Pin<Box<dyn Stream<Item = Block<Tx>> + Send>>,
    },
    #[allow(dead_code)]
    Approve {
        qc: Qc,
        block: consensus_engine::Block,
        votes: HashSet<Vote>,
    },
    LocalTimeout,
    NewView {
        timeout_qc: TimeoutQc,
        new_views: HashSet<NewView>,
    },
    TimeoutQc {
        timeout_qc: TimeoutQc,
    },
    RootTimeout {
        timeouts: HashSet<Timeout>,
    },
    ProposeBlock {
        qc: Qc,
    },
    None,
}
