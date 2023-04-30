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
use std::collections::{BTreeMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;
use std::time::Duration;
// crates
use futures::StreamExt;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::select;
// internal
use crate::network::messages::{NewViewMsg, VoteMsg};
use crate::network::NetworkAdapter;
use crate::tally::CarnotTally;
use consensus_engine::{
    Carnot, Committee, NewView, Overlay, Qc, StandardQc, Timeout, TimeoutQc, Vote,
};
use nomos_core::block::Block;
use nomos_core::crypto::PublicKey;
use nomos_core::fountain::FountainCode;
use nomos_core::staking::Stake;
use nomos_core::tx::Transaction;
use nomos_core::vote::Tally;
use nomos_mempool::{backend::MemPool, network::NetworkAdapter as MempoolAdapter, MempoolService};
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

pub struct CarnotConsensus<A, P, M, F, T, O>
where
    F: FountainCode,
    A: NetworkAdapter,
    M: MempoolAdapter<Tx = P::Tx>,
    P: MemPool,
    T: Tally,
    T::Qc: Clone,
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
    _tally: std::marker::PhantomData<T>,
    _overlay: std::marker::PhantomData<O>,
}

impl<A, P, M, F, T, O> ServiceData for CarnotConsensus<A, P, M, F, T, O>
where
    F: FountainCode,
    A: NetworkAdapter,
    P: MemPool,
    T: Tally,
    T::Qc: Clone,
    P::Tx: Transaction + Debug,
    <P::Tx as Transaction>::Hash: Debug,
    M: MempoolAdapter<Tx = P::Tx>,
    O: Overlay,
{
    const SERVICE_ID: ServiceId = "Carnot";
    type Settings = CarnotSettings<F, T>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NoMessage;
}

#[async_trait::async_trait]
impl<A, P, M, F, T, O> ServiceCore for CarnotConsensus<A, P, M, F, T, O>
where
    F: FountainCode + Send + Sync + 'static,
    A: NetworkAdapter + Send + Sync + 'static,
    P: MemPool + Send + Sync + 'static,
    T: Tally + Send + Sync + 'static,
    T::Settings: Send + Sync + 'static,
    T::Outcome: Send + Sync,
    T::Qc: Clone + Send + Sync,
    P::Settings: Send + Sync + 'static,
    P::Tx: Debug + Clone + serde::de::DeserializeOwned + Send + Sync + 'static,
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
            _tally: Default::default(),
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
        let tally = T::new(tally_settings);
        let overlay = O::new();
        // let leadership = Leadership::<P::Tx>::new(private_key, mempool_relay.clone());
        // FIXME: this should be taken from config
        let mut cur_view = View {
            seed: [0; 32],
            staking_keys: BTreeMap::new(),
            view_n: 0,
        };

        let genesis = consensus_engine::Block {
            id: [0; 32],
            view: 0,
            parent_qc: Qc::Standard(StandardQc::genesis()),
        };
        let mut carnot = Carnot::from_genesis(private_key, genesis, overlay);

        loop {
            // if we want to process multiple views at the same time this can
            // be spawned as a separate future

            // FIXME: this should probably have a timer to detect failed rounds
            let res = cur_view
                .resolve::<A, O, _, _, P::Tx>(
                    private_key,
                    &tip,
                    &network_adapter,
                    &fountain,
                    &tally,
                    &mut carnot,
                )
                .await;
        }
    }
}

#[derive(Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct Approval;

// Consensus round, also aids in guaranteeing synchronization
// between various data structures by means of lifetimes
pub struct View {
    seed: Seed,
    staking_keys: BTreeMap<NodeId, Stake>,
    pub view_n: consensus_engine::View,
}

impl View {
    // TODO: might want to encode steps in the type system
    pub async fn resolve<'view, A, O, F, T, Tx>(
        &'view self,
        node_id: NodeId,
        tip: &Tip,
        adapter: &A,
        fountain: &F,
        tally: &T,
        carnot: &mut Carnot<O>,
    ) -> Result<(Block<Tx::Hash>, View), Box<dyn std::error::Error + Send + Sync + 'static>>
    where
        A: NetworkAdapter + Send + Sync + 'static,
        F: FountainCode,
        Tx: Transaction,
        Tx::Hash: Debug,
        T: Tally + Send + Sync + 'static,
        T::Outcome: Send + Sync,
        T::Qc: Clone,
        O: Overlay,
    {
        unimplemented!()
    }
}

pub enum CarnotEvent<Tx: Clone + Eq + Hash> {
    Timeout,
    TimeoutDetected(HashSet<Timeout>),
    Proposal(Block<Tx>),
    TimeoutQc(TimeoutQc),
    NewView(HashSet<NewView>),
    Votes((Qc, HashSet<Vote>)),
}

pub struct CarnotEventBuilder<'view, A, F> {
    timeout: Duration,
    threshold: usize,
    view: consensus_engine::View,
    committee: Committee,
    adapter: &'view A,
    fountain: &'view F,
    tally: &'view CarnotTally,
}

impl<'view, A, F> CarnotEventBuilder<'view, A, F>
where
    A: NetworkAdapter,
    F: FountainCode,
{
    pub fn new(
        view: consensus_engine::View,
        committee: Committee,
        timeout: Duration,
        threshold: usize,
        adapter: &'view A,
        fountain: &'view F,
        tally: &'view CarnotTally,
    ) -> Self {
        Self {
            view,
            timeout,
            threshold,
            adapter,
            fountain,
            tally,
            committee,
        }
    }

    pub async fn next_proposal<Tx: Clone + Eq + Hash + Serialize + DeserializeOwned>(
        &self,
    ) -> CarnotEvent<Tx> {
        let proposal_stream = self
            .fountain
            .decode(self.adapter.proposal_chunks_stream(self.view).await);
        select! {
            Ok(proposal) = proposal_stream => {
                let block = Block::from_bytes(&proposal);
                CarnotEvent::Proposal(block)
            }
            _ = tokio::time::sleep(self.timeout) => {
                CarnotEvent::Timeout
            }
        }
    }

    pub async fn next_timeout_qc<Tx: Clone + Eq + Hash + Serialize + DeserializeOwned>(
        &self,
    ) -> Option<CarnotEvent<Tx>> {
        self.adapter
            .timeout_qc_stream(self.view)
            .await
            .next()
            .await
            .map(|qc| CarnotEvent::TimeoutQc(qc.qc))
    }

    pub async fn gather_votes<Tx: Clone + Eq + Hash + Serialize + DeserializeOwned>(
        &self,
    ) -> Option<CarnotEvent<Tx>> {
        let votes_stream = self
            .adapter
            .votes_stream::<VoteMsg>(&self.committee, self.view)
            .await;
        match self.tally.tally(self.view, votes_stream).await {
            Ok((qc, outcome)) => Some(CarnotEvent::Votes((qc, outcome))),
            Err(e) => {
                todo!("Handle tally error");
            }
        }
    }

    pub async fn gather_new_view<Tx: Clone + Eq + Hash + Serialize + DeserializeOwned>(
        &self,
    ) -> CarnotEvent<Tx> {
        // TODO: Maybe implement tally for unhappy path?
        let mut seen = HashSet::new();
        let mut votes = HashSet::new();
        let mut stream = self
            .adapter
            .votes_stream::<NewViewMsg>(&self.committee, self.view)
            .await;
        while let Some(msg) = StreamExt::next(&mut stream).await {
            if seen.contains(&msg.voter) {
                continue;
            }
            seen.insert(msg.voter);
            votes.insert(msg.vote);
            if votes.len() >= self.threshold {
                break;
            }
        }
        CarnotEvent::NewView(votes)
    }

    pub async fn gather_timeout<Tx: Clone + Eq + Hash + Serialize + DeserializeOwned>(
        &self,
    ) -> CarnotEvent<Tx> {
        let mut stream = self
            .adapter
            .timeout_stream(&self.committee, self.view)
            .await;
        CarnotEvent::TimeoutDetected(
            stream
                .take(self.threshold)
                .map(|msg| msg.vote)
                .collect()
                .await,
        )
    }
}
