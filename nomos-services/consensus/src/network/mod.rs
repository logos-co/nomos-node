pub mod adapters;
mod messages;

use crate::{Approval, Block};
use futures::Stream;
use nomos_network::backends::NetworkBackend;
use nomos_network::NetworkService;
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;

pub trait NetworkAdapter {
    type Backend: NetworkBackend + Send + Sync + 'static;
    fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;

    fn proposals_stream(&self) -> Box<dyn Stream<Item = Block>>;
    fn broadcast_block(&self, block: Block);
    fn approvals_stream(&self) -> Box<dyn Stream<Item = Approval>>;
    fn forward_approval(&self, approval: Approval);
}
