use bytes::Bytes;

use nomos_network::{
    backends::mock::{EventKind, Mock, NetworkEvent},
    NetworkMsg, NetworkService,
};
use overwatch_rs::services::{relay::OutboundRelay, ServiceData};
use tokio_stream::Stream;

use crate::{
    network::{
        messages::{ApprovalMsg, ProposalChunkMsg},
        NetworkAdapter,
    },
    Approval, View,
};

pub struct MockAdapter<D = rand::distributions::Standard>
where
    D: rand::distributions::Distribution<usize> + core::fmt::Debug + Clone + Send + Sync + 'static,
{
    network_relay: OutboundRelay<<NetworkService<Mock<D>> as ServiceData>::Message>,
}

impl<D> MockAdapter<D>
where
    D: rand::distributions::Distribution<usize> + core::fmt::Debug + Clone + Send + Sync + 'static,
{
    async fn message_subscriber_channel(
        &self,
    ) -> Result<
        tokio::sync::broadcast::Receiver<NetworkEvent>,
        tokio::sync::oneshot::error::RecvError,
    > {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        if let Err((_, _e)) = self
            .network_relay
            .send(NetworkMsg::Subscribe {
                kind: EventKind::Message,
                sender,
            })
            .await
        {
            todo!("log error");
        };
        receiver.await
    }
}

#[async_trait::async_trait]
impl<D> NetworkAdapter for MockAdapter<D>
where
    D: rand::distributions::Distribution<usize> + core::fmt::Debug + Clone + Send + Sync + 'static,
{
    type Backend = Mock<D>;

    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        Self { network_relay }
    }

    async fn proposal_chunks_stream(&self) -> Box<dyn Stream<Item = Bytes> + Send + Sync + Unpin> {
        todo!()
    }

    async fn broadcast_block_chunk(&self, _view: &View, _chunk_message: ProposalChunkMsg) {
        todo!()
    }

    async fn approvals_stream(&self) -> Box<dyn Stream<Item = Approval>> {
        todo!()
    }

    async fn forward_approval(&self, _approval_message: ApprovalMsg) {
        todo!()
    }
}
