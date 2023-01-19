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
use std::collections::{BTreeMap, HashSet};
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
            // TODO: add leadership module
            view.resolve::<A, Member<'_, COMMITTEE_SIZE>, _, _, _>(
                private_key,
                &tip,
                &network_adapter,
                &fountain,
                &leadership,
            )
            .await;
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
    const APPROVAL_THRESHOLD: usize = 1;

    // TODO: might want to encode steps in the type system
    async fn resolve<'view, A, O, F, Tx, Id>(
        &'view self,
        node_id: NodeId,
        tip: &Tip,
        adapter: &A,
        fountain: &F,
        leadership: &Leadership<Tx, Id>,
    ) where
        A: NetworkAdapter + Send + Sync + 'static,
        F: FountainCode,
        O: Overlay<'view, A, F>,
    {
        let overlay = O::new(self, node_id);

        let block = if let LeadershipResult::Leader { block, .. } =
            leadership.try_propose_block(self, tip).await
        {
            Ok(block)
        } else {
            overlay.reconstruct_proposal_block(adapter, fountain).await
        };
        match block {
            Ok(block) => {
                // TODO: verify?
                overlay
                    .broadcast_block(block.clone(), adapter, fountain)
                    .await;
                self.approve(&overlay, block, adapter).await;
            }
            Err(_e) => {
                // TODO: log error
            }
        }
    }

    async fn approve<
        'view,
        Network: NetworkAdapter,
        Fountain: FountainCode,
        O: Overlay<'view, Network, Fountain>,
    >(
        &'view self,
        overlay: &O,
        block: Block,
        adapter: &Network,
    ) {
        // wait for approval in the overlay, if necessary
        let mut approvals = HashSet::new();
        let mut stream = overlay.collect_approvals(block, adapter).await;
        while let Some(approval) = stream.recv().await {
            approvals.insert(approval);
            if approvals.len() > Self::APPROVAL_THRESHOLD {
                let self_approval = self.craft_proof_of_approval(approvals.into_iter());
                overlay.forward_approval(self_approval, adapter).await;
                return;
            }
        }
    }

    fn craft_proof_of_approval(&self, _approvals: impl Iterator<Item = Approval>) -> Approval {
        todo!()
    }

    pub fn is_leader(&self, _node_id: NodeId) -> bool {
        todo!()
    }
}
