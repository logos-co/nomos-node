use nomos_core::block::BlockChunk;
use nomos_network::{
    backends::mock::{EventKind, Mock, MockBackendMessage, MockMessage, NetworkEvent},
    NetworkMsg, NetworkService,
};
use overwatch_rs::services::{relay::OutboundRelay, ServiceData};
use tokio_stream::Stream;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

use crate::network::{messages::ApprovalMsg, NetworkAdapter};

const CHUNK_SIZE: usize = 8;

pub struct MockAdapter {
    network_relay: OutboundRelay<<NetworkService<Mock> as ServiceData>::Message>,
}

impl MockAdapter {
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
impl NetworkAdapter for MockAdapter {
    type Backend = Mock;

    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        Self { network_relay }
    }

    async fn proposal_chunks_stream(&self) -> Box<dyn Stream<Item = BlockChunk>> {
        // let stream_channel = self
        //     .message_subscriber_channel()
        //     .await
        //     .unwrap_or_else(|_e| todo!("handle error"));
        // Box::new(
        //     BroadcastStream::new(stream_channel).filter_map(|msg| async move {
        //         match msg {
        //             Ok(NetworkEvent::RawMessage(message)) => {
        //                 todo!()
        //             }
        //             Err(_e) => None,
        //         }
        //     }),
        // )
        todo!()
    }

    async fn broadcast_block_chunk(&self, _view: View, chunk_message: ProposalChunkMsg) {
        // TODO: probably later, depending on the view we should map to different content topics
        // but this is an ongoing idea that should/will be discus.
        // let message = MockMessage::new::<[u8; CHUNK_SIZE]>(
        //     chunk_message.as_bytes(),
        //     WAKU_CARNOT_BLOCK_CONTENT_TOPIC.clone(),
        //     1,
        //     chrono::Utc::now().timestamp() as usize,
        // );
        // if let Err((_, _e)) = self
        //     .network_relay
        //     .send(NetworkMsg::Process(MockBackendMessage::Broadcast {
        //         message,
        //         topic: Some(WAKU_CARNOT_PUB_SUB_TOPIC.clone()),
        //     }))
        //     .await
        // {
        //     todo!("log error");
        // };
        todo!()
    }

    async fn approvals_stream(&self) -> Box<dyn Stream<Item = Approval>> {
        // let stream_channel = self
        //     .message_subscriber_channel()
        //     .await
        //     .unwrap_or_else(|_e| todo!("handle error"));
        // Box::new(
        //     BroadcastStream::new(stream_channel).filter_map(|msg| async move {
        //         match msg {
        //             Ok(event) => match event {
        //                 NetworkEvent::RawMessage(message) => {
        //                     // TODO: this should actually check the whole content topic,
        //                     // waiting for this [PR](https://github.com/waku-org/waku-rust-bindings/pull/28)
        //                     if WAKU_CARNOT_APPROVAL_CONTENT_TOPIC.content_topic_name
        //                         == message.content_topic().content_topic_name
        //                     {
        //                         let payload = message.payload();
        //                         Some(ApprovalMsg::from_bytes(payload).approval)
        //                     } else {
        //                         None
        //                     }
        //                 }
        //             },
        //             Err(_e) => None,
        //         }
        //     }),
        // )
        todo!()
    }

    async fn forward_approval(&self, approval_message: ApprovalMsg) {
        // let message = MockMessage::new(
        //     approval_message.as_bytes(),
        //     WAKU_CARNOT_APPROVAL_CONTENT_TOPIC.clone(),
        //     1,
        //     chrono::Utc::now().timestamp() as usize,
        // );
        // if let Err((_, _e)) = self
        //     .network_relay
        //     .send(NetworkMsg::Process(MockBackendMessage::Broadcast {
        //         message,
        //         topic: Some(WAKU_CARNOT_PUB_SUB_TOPIC.clone()),
        //     }))
        //     .await
        // {
        //     todo!("log error");
        // };
        todo!()
    }
}
