//! In this module, and children ones, the 'view lifetime is tied to a logical consensus view,
//! represented by the `View` struct.
//! This is done to ensure that all the different data structs used to represent various actors
//! are always synchronized (i.e. it cannot happen that we accidentally use committees from different views).
//! It's obviously extremely important that the information contained in `View` is synchronized across different
//! nodes, but that has to be achieved through different means.
mod leadership;
mod network;
pub mod overlay;
mod tip;

// std
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Debug;
// crates
// internal
use crate::network::NetworkAdapter;
use leadership::{Leadership, LeadershipResult};
use nomos_core::block::Block;
use nomos_core::crypto::PublicKey;
use nomos_core::fountain::FountainCode;
use nomos_core::staking::Stake;
use nomos_mempool::{backend::MemPool, network::NetworkAdapter as MempoolAdapter, MempoolService};
use nomos_network::NetworkService;
use overlay::{Member, Overlay};
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

const COMMITTEE_SIZE: usize = 1;

pub struct CarnotSettings<Fountain: FountainCode> {
    private_key: [u8; 32],
    fountain_settings: Fountain::Settings,
}

impl<Fountain: FountainCode> Clone for CarnotSettings<Fountain> {
    fn clone(&self) -> Self {
        Self {
            private_key: self.private_key,
            fountain_settings: self.fountain_settings.clone(),
        }
    }
}

pub struct CarnotConsensus<A, P, M, F>
where
    F: FountainCode,
    A: NetworkAdapter,
    M: MempoolAdapter<Tx = P::Tx>,
    P: MemPool,
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
}

impl<A, P, M, F> ServiceData for CarnotConsensus<A, P, M, F>
where
    F: FountainCode,
    A: NetworkAdapter,
    P: MemPool,
    P::Tx: Debug,
    P::Id: Debug,
    M: MempoolAdapter<Tx = P::Tx>,
{
    const SERVICE_ID: ServiceId = "Carnot";
    type Settings = CarnotSettings<F>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NoMessage;
}

#[async_trait::async_trait]
impl<A, P, M, F> ServiceCore for CarnotConsensus<A, P, M, F>
where
    F: FountainCode + Send + Sync + 'static,
    A: NetworkAdapter + Send + Sync + 'static,
    P: MemPool + Send + Sync + 'static,
    P::Settings: Send + Sync + 'static,
    P::Tx: Debug + Send + Sync + 'static,
    P::Id: Debug + Send + Sync + 'static,
    M: MempoolAdapter<Tx = P::Tx> + Send + Sync + 'static,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        let network_relay = service_state.overwatch_handle.relay();
        let mempool_relay = service_state.overwatch_handle.relay();
        Ok(Self {
            service_state,
            network_relay,
            _fountain: Default::default(),
            mempool_relay,
        })
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
        let mut view_generator = self.view_generator().await;

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
        } = self.service_state.settings_reader.get_updated_settings();

        let network_adapter = A::new(network_relay).await;

        let tip = Tip;

        let fountain = F::new(fountain_settings);

        let leadership = Leadership::new(private_key, mempool_relay);
        loop {
            let view = view_generator.next().await;
            // if we want to process multiple views at the same time this can
            // be spawned as a separate future

            // FIXME: this should probably have a timer to detect failed rounds
            let res = view
                .resolve::<A, Member<'_, COMMITTEE_SIZE>, _, _, _>(
                    private_key,
                    &tip,
                    &network_adapter,
                    &fountain,
                    &leadership,
                )
                .await;
            match res {
                Ok(_block) => {
                    // resolved block, mark as verified and possibly update the tip
                    // not sure what mark as verified means, e.g. if we want an event subscription
                    // system for this to be used for example by the ledger, storage and mempool
                }
                Err(e) => {
                    tracing::error!("Error while resolving view: {}", e);
                }
            }
        }
    }
}

impl<A, P, M, F> CarnotConsensus<A, P, M, F>
where
    F: FountainCode + Send + Sync + 'static,
    A: NetworkAdapter + Send + Sync + 'static,
    P: MemPool + Send + Sync + 'static,
    P::Settings: Clone + Send + Sync + 'static,
    P::Tx: Debug + Send + Sync + 'static,
    P::Id: Debug + Send + Sync + 'static,
    M: MempoolAdapter<Tx = P::Tx> + Send + Sync + 'static,
{
    // Build a service that generates new views as they become available
    async fn view_generator(&self) -> ViewGenerator {
        todo!()
    }
}

/// Tracks new views and make them available as soon as they are available
///
/// A new view is normally generated as soon a a block is approved, but
/// additional logic is needed in failure cases, like when no new block is
/// approved for a long enough period of time
struct ViewGenerator;

impl ViewGenerator {
    async fn next(&mut self) -> View {
        todo!()
    }
}

#[derive(Hash, Eq, PartialEq)]
pub struct Approval;

// Consensus round, also aids in guaranteeing synchronization
// between various data structures by means of lifetimes
pub struct View {
    seed: Seed,
    staking_keys: BTreeMap<NodeId, Stake>,
    _view_n: u64,
}

impl View {
    // TODO: might want to encode steps in the type system
    async fn resolve<'view, A, O, F, Tx, Id>(
        &'view self,
        node_id: NodeId,
        tip: &Tip,
        adapter: &A,
        fountain: &F,
        leadership: &Leadership<Tx, Id>,
    ) -> Result<Block, Box<dyn Error>>
    where
        A: NetworkAdapter + Send + Sync + 'static,
        F: FountainCode,
        O: Overlay<'view, A, F>,
    {
        let overlay = O::new(self, node_id);
        // FIXME: this is still a working in progress and best-of-my-understanding
        //       of the consensus protocol, having pseudocode would be very helpful

        // Consensus in Carnot is achieved in 4 steps from the point of view of a node:
        // 1) The node receives a block proposal from a leader and verifies it
        // 2, The node signals to the network its approval for the block.
        //    Depending on the overlay, this may require waiting for a certain number
        //    of other approvals.
        // 3) The node waits for consensus to be reached and mark the block as verified
        // 4) Upon verifing a block B, if B.parent = B' and B'.parent = B'' and
        //    B'.view = B''.view + 1, then the node commits B''.
        //    This happens implicitly at the chain level and does not require any
        //    explicit action from the node.

        // 1) Collect and verify block proposal.
        //    If this node is the leader this is trivial
        let block = if let LeadershipResult::Leader { block, .. } =
            leadership.try_propose_block(self, tip).await
        {
            block
        } else {
            overlay
                .reconstruct_proposal_block(adapter, fountain)
                .await?
            // TODO: reshare the block?
            // TODO: verify
        };

        // 2) Signal approval to the network
        overlay.approve_and_forward(&block, adapter).await?;

        // 3) Wait for consensus
        overlay.wait_for_consensus(&block, adapter).await;

        Ok(block)
    }

    pub fn is_leader(&self, _node_id: NodeId) -> bool {
        todo!()
    }
}
