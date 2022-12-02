use crate::network::NetworkAdapter;
use crate::{Approval, Block, BlockChunk, View};
use async_trait::async_trait;
use futures::Stream;
use nomos_network::backends::waku::{NetworkEvent, WakuBackendMessage};
use nomos_network::backends::{
    waku::{EventKind, Waku},
    NetworkBackend,
};
use nomos_network::{NetworkMsg, NetworkService};
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;

const WAKU_CARNOT_CONTENT_TOPIC: &str = "CARNOT";

pub struct WakuAdapter {
    network_relay: OutboundRelay<<NetworkService<Waku> as ServiceData>::Message>,
    incoming_event_messages: tokio::sync::broadcast::Receiver<NetworkEvent>,
}

#[async_trait::async_trait]
impl NetworkAdapter for WakuAdapter {
    type Backend = Waku;

    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        let (sender, mut receiver) = tokio::sync::oneshot::channel();
        let msg = NetworkMsg::Subscribe {
            kind: EventKind::Message,
            sender,
        };
        let incoming_event_messages = receiver
            .try_recv()
            .expect("Subscription to waku messages to succeed");
        network_relay
            .send(msg)
            .await
            .expect("Network service should be responsive");
        Self {
            network_relay,
            incoming_event_messages,
        }
    }

    async fn proposal_chunks_stream(&self) -> Box<dyn Stream<Item = BlockChunk>> {
        todo!()
    }

    async fn broadcast_block_chunk(&self, view: View, block: BlockChunk) {
        todo!()
    }

    async fn approvals_stream(&self) -> Box<dyn Stream<Item = Approval>> {
        todo!()
    }

    async fn forward_approval(&self, approval: Approval) {
        todo!()
    }
}
