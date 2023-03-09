use bytes::Bytes;
use futures::StreamExt;
use nomos_network::{
    backends::mock::{
        EventKind, Mock, MockBackendMessage, MockContentTopic, MockMessage, NetworkEvent,
    },
    NetworkMsg, NetworkService,
};
use overwatch_rs::services::{relay::OutboundRelay, ServiceData};
use tokio_stream::{wrappers::BroadcastStream, Stream};

use crate::{
    network::{
        messages::{ApprovalMsg, ProposalChunkMsg},
        NetworkAdapter,
    },
    overlay::committees::Committee,
    Approval, View,
};

const MOCK_PUB_SUB_TOPIC: &str = "MockPubSubTopic";
const MOCK_BLOCK_CONTENT_TOPIC: MockContentTopic = MockContentTopic::new("MockSim", 1, "MockBlock");
const MOCK_APPROVAL_CONTENT_TOPIC: MockContentTopic =
    MockContentTopic::new("MockSim", 1, "MockApproval");

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
        if let Err((e, _e)) = self
            .network_relay
            .send(NetworkMsg::Subscribe {
                kind: EventKind::Message,
                sender,
            })
            .await
        {
            tracing::error!("error subscribing to network messages: {:?}", e);
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

    async fn proposal_chunks_stream(
        &self,
        _committee: Committee,
        _view: &View,
    ) -> Box<dyn Stream<Item = Bytes> + Send + Sync + Unpin> {
        let stream_channel = self
            .message_subscriber_channel()
            .await
            .map_err(|e| tracing::error!("handle error {e:?}"))
            .unwrap();
        Box::new(BroadcastStream::new(stream_channel).filter_map(|msg| {
            Box::pin(async move {
                match msg {
                    Ok(event) => match event {
                        NetworkEvent::RawMessage(message) => {
                            if MOCK_BLOCK_CONTENT_TOPIC.content_topic_name
                                == message.content_topic().content_topic_name
                            {
                                let payload = message.payload();
                                Some(ProposalChunkMsg::from_bytes(payload.as_bytes()).chunk)
                            } else {
                                None
                            }
                        }
                    },
                    Err(_e) => None,
                }
            })
        }))
    }

    async fn broadcast_block_chunk(
        &self,
        _committee: Committee,
        _view: &View,
        chunk_message: ProposalChunkMsg,
    ) {
        let message = MockMessage::new(
            String::from_utf8_lossy(chunk_message.as_bytes()).to_string(),
            MOCK_BLOCK_CONTENT_TOPIC,
            1,
            chrono::Utc::now().timestamp() as usize,
        );
        if let Err((e, _)) = self
            .network_relay
            .send(NetworkMsg::Process(MockBackendMessage::Broadcast {
                msg: message,
                topic: MOCK_PUB_SUB_TOPIC.to_string(),
            }))
            .await
        {
            tracing::error!("Failed to broadcast block chunk: {:?}", e);
        };
    }

    async fn approvals_stream(
        &self,
        _committee: Committee,
        _view: &View,
    ) -> Box<dyn Stream<Item = Approval> + Send> {
        let stream_channel = self
            .message_subscriber_channel()
            .await
            .unwrap_or_else(|_e| todo!("handle error"));
        Box::new(
            BroadcastStream::new(stream_channel).filter_map(|msg| async move {
                match msg {
                    Ok(event) => match event {
                        NetworkEvent::RawMessage(message) => {
                            if MOCK_APPROVAL_CONTENT_TOPIC.content_topic_name
                                == message.content_topic().content_topic_name
                            {
                                let payload = message.payload();
                                Some(ApprovalMsg::from_bytes(payload.as_bytes()).approval)
                            } else {
                                None
                            }
                        }
                    },
                    Err(_e) => None,
                }
            }),
        )
    }

    async fn forward_approval(
        &self,
        _committee: Committee,
        _view: &View,
        approval_message: ApprovalMsg,
    ) {
        let message = MockMessage::new(
            String::from_utf8_lossy(&approval_message.as_bytes()).to_string(),
            MOCK_APPROVAL_CONTENT_TOPIC,
            1,
            chrono::Utc::now().timestamp() as usize,
        );
        if let Err((e, _e)) = self
            .network_relay
            .send(NetworkMsg::Process(MockBackendMessage::Broadcast {
                msg: message,
                topic: MOCK_PUB_SUB_TOPIC.to_string(),
            }))
            .await
        {
            tracing::error!("Failed to forward approval: {:?}", e);
        };
    }
}
