//! In this module, and children ones, the 'view lifetime is tied to a logical consensus view,
//! represented by the `View` struct.
//! This is done to ensure that all the different data structs used to represent various actors
//! are always synchronized (i.e. it cannot happen that we accidentally use committees from different views).
//! It's obviously extremely important that the information contained in `View` is synchronized across different
//! nodes, but that has to be achieved through different means.
mod leadership;
mod network;
pub mod overlay;
#[cfg(test)]
mod test;
mod tip;

// std
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Debug;
// crates
use serde::{Deserialize, Serialize};
// internal
use crate::network::NetworkAdapter;
use leadership::{Leadership, LeadershipResult};
use nomos_core::block::Block;
use nomos_core::crypto::PublicKey;
use nomos_core::fountain::FountainCode;
use nomos_core::staking::Stake;
use nomos_core::vote::Tally;
use nomos_mempool::{backend::MemPool, network::NetworkAdapter as MempoolAdapter, MempoolService};
use nomos_network::NetworkService;
use overlay::Overlay;
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
    O: Overlay<A, F, T>,
    P::Tx: Debug + 'static,
    P::Id: Debug + 'static,
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
    P::Tx: Debug,
    P::Id: Debug,
    M: MempoolAdapter<Tx = P::Tx>,
    O: Overlay<A, F, T>,
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
    P::Settings: Send + Sync + 'static,
    P::Tx: Debug + Send + Sync + 'static,
    P::Id: Debug + Send + Sync + 'static,
    M: MempoolAdapter<Tx = P::Tx> + Send + Sync + 'static,
    O: Overlay<A, F, T> + Send + Sync + 'static,
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

        let leadership = Leadership::new(private_key, mempool_relay);
        // FIXME: this should be taken from config
        let mut cur_view = View {
            seed: [0; 32],
            staking_keys: BTreeMap::new(),
            view_n: 0,
        };
        loop {
            // if we want to process multiple views at the same time this can
            // be spawned as a separate future

            // FIXME: this should probably have a timer to detect failed rounds
            let res = cur_view
                .resolve::<A, O, _, _, _, _>(
                    private_key,
                    &tip,
                    &network_adapter,
                    &fountain,
                    &tally,
                    &leadership,
                )
                .await;
            match res {
                Ok((_block, view)) => {
                    // resolved block, mark as verified and possibly update the tip
                    // not sure what mark as verified means, e.g. if we want an event subscription
                    // system for this to be used for example by the ledger, storage and mempool
                    cur_view = view;
                }
                Err(e) => {
                    tracing::error!("Error while resolving view: {}", e);
                }
            }
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
    pub view_n: u64,
}

impl View {
    // TODO: might want to encode steps in the type system
    pub async fn resolve<'view, A, O, F, T, Tx, Id>(
        &'view self,
        node_id: NodeId,
        tip: &Tip,
        adapter: &A,
        fountain: &F,
        tally: &T,
        leadership: &Leadership<Tx, Id>,
    ) -> Result<(Block, View), Box<dyn Error>>
    where
        A: NetworkAdapter + Send + Sync + 'static,
        F: FountainCode,
        T: Tally + Send + Sync + 'static,
        T::Outcome: Send + Sync,
        O: Overlay<A, F, T>,
    {
        let res = if self.is_leader(node_id) {
            let block = self
                .resolve_leader::<A, O, F, T, _, _>(
                    node_id, tip, adapter, fountain, tally, leadership,
                )
                .await
                .unwrap(); // FIXME: handle sad path
            let next_view = self.generate_next_view(&block);
            (block, next_view)
        } else {
            self.resolve_non_leader::<A, O, F, T>(node_id, adapter, fountain, tally)
                .await
                .unwrap() // FIXME: handle sad path
        };

        // Commit phase:
        // Upon verifing a block B, if B.parent = B' and B'.parent = B'' and
        //    B'.view = B''.view + 1, then the node commits B''.
        //    This happens implicitly at the chain level and does not require any
        //    explicit action from the node.

        Ok(res)
    }

    async fn resolve_leader<'view, A, O, F, T, Tx, Id>(
        &'view self,
        node_id: NodeId,
        tip: &Tip,
        adapter: &A,
        fountain: &F,
        tally: &T,
        leadership: &Leadership<Tx, Id>,
    ) -> Result<Block, ()>
    where
        A: NetworkAdapter + Send + Sync + 'static,
        F: FountainCode,
        T: Tally + Send + Sync + 'static,
        T::Outcome: Send + Sync,
        O: Overlay<A, F, T>,
    {
        let overlay = O::new(self, node_id);

        // We need to build the QC for the block we are proposing
        let qc = overlay.build_qc(self, adapter, tally).await;

        let LeadershipResult::Leader { block, _view }  = leadership
            .try_propose_block(self, tip, qc)
            .await else { panic!("we are leader")};

        overlay
            .broadcast_block(self, block.clone(), adapter, fountain)
            .await;

        Ok(block)
    }

    async fn resolve_non_leader<'view, A, O, F, T>(
        &'view self,
        node_id: NodeId,
        adapter: &A,
        fountain: &F,
        tally: &T,
    ) -> Result<(Block, View), ()>
    where
        A: NetworkAdapter + Send + Sync + 'static,
        F: FountainCode,
        T: Tally + Send + Sync + 'static,
        O: Overlay<A, F, T>,
    {
        let overlay = O::new(self, node_id);
        // Consensus in Carnot is achieved in 2 steps from the point of view of a node:
        // 1) The node receives a block proposal from a leader and verifies it
        // 2) The node signals to the network its approval for the block.
        //    Depending on the overlay, this may require waiting for a certain number
        //    of other approvals.

        // 1) Collect and verify block proposal.
        let block = overlay
            .reconstruct_proposal_block(self, adapter, fountain)
            .await
            .unwrap(); // FIXME: handle sad path

        // TODO: verify
        // TODO: reshare the block?
        let next_view = self.generate_next_view(&block);

        // 2) Signal approval to the network
        // We only consider the happy path for now
        if self.pipelined_safe_block(&block) {
            overlay
                .approve_and_forward(self, &block, adapter, tally, &next_view)
                .await
                .unwrap(); // FIXME: handle sad path
        }

        Ok((block, next_view))
    }

    pub fn is_leader(&self, _node_id: NodeId) -> bool {
        false
    }

    pub fn id(&self) -> u64 {
        self.view_n
    }

    // Verifies the block is new and the previous leader did not fail
    fn pipelined_safe_block(&self, _: &Block) -> bool {
        // return b.view_n >= self.view_n && b.view_n == b.qc.view_n
        true
    }

    fn generate_next_view(&self, _b: &Block) -> View {
        let mut seed = self.seed;
        seed[0] += 1;
        View {
            seed,
            staking_keys: self.staking_keys.clone(),
            view_n: self.view_n + 1,
        }
    }
}
