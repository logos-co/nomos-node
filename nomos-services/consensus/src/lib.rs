/// In this module, and children ones, the 'view lifetime is tied to a logical consensus view,
/// represented by the `View` struct.
/// This is done to ensure that all the different data structs used to represent various actors
/// are always synchronized (i.e. it cannot happen that we accidentally use committees from different views).
/// It's obviously extremely important that the information contained in `View` is synchronized across different
/// nodes, but that has to be achieved through different means.
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

use std::collections::{BTreeMap, HashSet};

const COMMITTEE_SIZE: usize = 1;

#[derive(Clone)]
pub struct CarnotSettings {
    private_key: [u8; 32],
}

pub struct CarnotConsensus {
    service_state: ServiceStateHandle<Self>,
}

impl ServiceData for CarnotConsensus {
    const SERVICE_ID: ServiceId = "Carnot";
    type Settings = CarnotSettings;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NoMessage;
}

#[async_trait::async_trait]
impl ServiceCore for CarnotConsensus {
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        Ok(Self { service_state })
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
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
            view.resolve::<Member<'_, COMMITTEE_SIZE>>(node_id).await;
        }
    }
}

impl CarnotConsensus {
    // get a stream of incoming blocks from the network
    async fn subscribe_to_incoming_blocks(&self) -> impl Stream<Item = Block> {
        futures::stream::empty()
    }

    // Build a service that generates new views as they become available
    //
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

#[derive(Clone)]
pub struct Block;
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
    async fn resolve<'view, O: Overlay<'view>>(&'view self, node_id: NodeId) {
        let overlay = O::new(self, node_id);

        let block = overlay.reconstruct_proposal_block().await;
        // TODO: verify?
        overlay.broadcast_block(block.clone()).await;
        self.approve(&overlay, block).await;
    }

    async fn approve<'view, O: Overlay<'view>>(&'view self, overlay: &O, block: Block) {
        // wait for approval in the overlay, if necessary
        let mut approvals = HashSet::new();
        let mut stream = overlay.collect_approvals(block).await;
        while let Some(approval) = stream.recv().await {
            approvals.insert(approval);
            if approvals.len() > Self::APPROVAL_THRESHOLD {
                let self_approval = self.craft_proof_of_approval(approvals.into_iter());
                overlay.forward_approval(self_approval).await;
                return;
            }
        }
    }

    fn craft_proof_of_approval(&self, _approvals: impl Iterator<Item = Approval>) -> Approval {
        todo!()
    }
}
