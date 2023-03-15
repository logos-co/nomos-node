use bytes::Bytes;
use futures::StreamExt;
use nomos_core::{tx::mock::MockTransaction, wire};
use nomos_mempool::network::adapters::mock::MOCK_TX_CONTENT_TOPIC;
use nomos_network::{
    backends::mock::{
        EventKind, Mock, MockBackendMessage, MockContentTopic, MockMessage, NetworkEvent,
    },
    NetworkMsg, NetworkService,
};
use overwatch_rs::services::{relay::OutboundRelay, ServiceData};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio_stream::{wrappers::BroadcastStream, Stream};

use crate::{
    network::{
        messages::{ProposalChunkMsg, VoteMsg},
        NetworkAdapter,
    },
    overlay::committees::Committee,
    View,
};

pub const MOCK_PUB_SUB_TOPIC: &str = "MockPubSubTopic";
pub const MOCK_BLOCK_CONTENT_TOPIC: MockContentTopic =
    MockContentTopic::new("MockSim", 1, "MockBlock");
pub const MOCK_APPROVAL_CONTENT_TOPIC: MockContentTopic =
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
                                Some(
                                    ProposalChunkMsg::from_bytes(
                                        &wire::serialize(&MockTransaction::from(message)).unwrap(),
                                    )
                                    .chunk,
                                )
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

    async fn votes_stream<Vote: DeserializeOwned>(
        &self,
        _committee: Committee,
        _view: &View,
    ) -> Box<dyn Stream<Item = Vote> + Send> {
        let stream_channel = self
            .message_subscriber_channel()
            .await
            .unwrap_or_else(|_e| todo!("handle error"));
        Box::new(
            BroadcastStream::new(stream_channel).filter_map(|msg| async move {
                match msg {
                    Ok(event) => match event {
                        NetworkEvent::RawMessage(message) => {
                            if message.content_topic == MOCK_TX_CONTENT_TOPIC {
                                let payload = message.payload();
                                let vmsg = VoteMsg::<Vote>::from_bytes(payload.as_bytes());
                                Some(vmsg.vote)
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

    async fn forward_approval<Vote: Serialize>(
        &self,
        _committee: Committee,
        _view: &View,
        approval_message: VoteMsg<Vote>,
    ) where
        Vote: Send,
    {
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
