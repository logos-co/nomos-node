use super::NetworkAdapter;
use crate::{Approval, Block};
use futures::Stream;
use nomos_network::backends::Waku;
use nomos_network::NetworkService;
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;

pub struct WakuAdapter {
    waku_backend: Waku,
}

impl NetworkAdapter for WakuAdapter {
    type Backend = Waku;

    fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        todo!()
    }

    fn proposals_stream(&self) -> Box<dyn Stream<Item = Block>> {
        todo!()
    }

    fn broadcast_block(&self, block: Block) {
        todo!()
    }

    fn approvals_stream(&self) -> Box<dyn Stream<Item = Approval>> {
        todo!()
    }

    fn forward_approval(&self, approval: Approval) {
        todo!()
    }
}
