//! In this module, and children ones, the 'view lifetime is tied to a logical consensus view,
//! represented by the `View` struct.
//! This is done to ensure that all the different data structs used to represent various actors
//! are always synchronized (i.e. it cannot happen that we accidentally use committees from different views).
//! It's obviously extremely important that the information contained in `View` is synchronized across different
//! nodes, but that has to be achieved through different means.
mod network;
pub mod overlay;

use futures::Stream;
use overlay::{Member, Overlay};
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::NoMessage,
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};

// Raw bytes for now, could be a ed25519 public key
pub type NodeId = [u8; 32];
// Random seed for each round provided by the protocol
pub type Seed = [u8; 32];
pub type Stake = u64;

use crate::network::NetworkAdapter;
use nomos_network::NetworkService;
use overwatch_rs::services::relay::{OutboundRelay, Relay};
use std::collections::{BTreeMap, HashSet};

const COMMITTEE_SIZE: usize = 1;

#[derive(Clone)]
pub struct CarnotSettings {
    private_key: [u8; 32],
}

pub struct CarnotConsensus<Network: NetworkAdapter + Send + Sync + 'static> {
    service_state: ServiceStateHandle<Self>,
    // underlying networking backend. We need this so we can relay and check the types properly
    // when implementing ServiceCore for CarnotConsensus
    network_relay: Relay<NetworkService<<Network as NetworkAdapter>::Backend>>,
}

impl<Network: NetworkAdapter + Send + Sync + 'static> ServiceData for CarnotConsensus<Network> {
    const SERVICE_ID: ServiceId = "Carnot";
    type Settings = CarnotSettings;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NoMessage;
}

#[async_trait::async_trait]
impl<Network: NetworkAdapter + Send + Sync + 'static> ServiceCore for CarnotConsensus<Network> {
    fn init(service_state: ServiceStateHandle<Self>) -> Self {
        let network_relay = service_state.overwatch_handle.relay();
        Self {
            service_state,
            network_relay,
        }
    }

    async fn run(mut self) {
        let network_relay: OutboundRelay<_> = self
            .network_relay
            .connect()
            .await
            .expect("Relay connection with NetworkService should succeed");

        let network_adapter = Network::new(network_relay);
        let block_stream = self.subscribe_to_incoming_blocks().await;
        // TODO: fix
        let node_id = self
            .service_state
            .settings_reader
            .get_updated_settings()
            .private_key;

        let mut view_generator = self.view_generator(block_stream).await;

        loop {
            let view = view_generator.next().await;
            // if we want to process multiple views at the same time this can
            // be spawned as a separate future
            // TODO: add leadership module
            view.resolve::<Network, Member<'_, COMMITTEE_SIZE>>(node_id, &network_adapter)
                .await;
        }
    }
}

impl<Network: NetworkAdapter + Send + Sync + 'static> CarnotConsensus<Network> {
    // get a stream of incoming blocks from the network
    async fn subscribe_to_incoming_blocks(&self) -> impl Stream<Item = Block> {
        futures::stream::empty()
    }

    // Build a service that generates new views as they become available
    async fn view_generator(&self, _block_stream: impl Stream<Item = Block>) -> ViewGenerator {
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

/// A block
#[derive(Clone)]
pub struct Block;

/// A block chunk, N pieces are necessary to reconstruct the full block
#[derive(Clone, Copy)]
pub struct BlockChunk;

impl Block {
    /// Fake implementation of erasure coding protocol
    pub fn chunk<const SIZE: usize>(self) -> [BlockChunk; SIZE] {
        [BlockChunk; SIZE]
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

    // TODO: might want to encode steps in the typesystem
    async fn resolve<'view, Network: NetworkAdapter, O: Overlay<'view, Network>>(
        &'view self,
        node_id: NodeId,
        adapter: &Network,
    ) {
        let overlay = O::new(self, node_id);

        let block = overlay.reconstruct_proposal_block(adapter).await;
        // TODO: verify?
        overlay.broadcast_block(block.clone(), adapter).await;
        self.approve(&overlay, block, adapter).await;
    }

    async fn approve<'view, Network: NetworkAdapter, O: Overlay<'view, Network>>(
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
}
