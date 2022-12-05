// std
// crates
use futures::{Stream, StreamExt};
use once_cell::sync::Lazy;
use tokio::sync::broadcast::Receiver;
use tokio::sync::oneshot::error::RecvError;
use tokio_stream::wrappers::BroadcastStream;
// internal
use crate::network::messages::{ApprovalMsg, ProposalChunkMsg};
use crate::network::NetworkAdapter;
use crate::{Approval, BlockChunk, View};
use nomos_network::backends::waku::{EventKind, Waku};
use nomos_network::backends::waku::{NetworkEvent, WakuBackendMessage};
use nomos_network::{NetworkMsg, NetworkService};
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;

use waku_bindings::{Encoding, WakuContentTopic, WakuMessage, WakuPubSubTopic};

static WAKU_CARNOT_PUB_SUB_TOPIC: Lazy<WakuPubSubTopic> =
    Lazy::new(|| WakuPubSubTopic::new("CarnotSim".to_string(), Encoding::Proto));

static WAKU_CARNOT_BLOCK_CONTENT_TOPIC: Lazy<WakuContentTopic> = Lazy::new(|| WakuContentTopic {
    application_name: "CarnotSim".to_string(),
    version: 1,
    content_topic_name: "CarnotBlock".to_string(),
    encoding: Encoding::Proto,
});

static WAKU_CARNOT_APPROVAL_CONTENT_TOPIC: Lazy<WakuContentTopic> =
    Lazy::new(|| WakuContentTopic {
        application_name: "CarnotSim".to_string(),
        version: 1,
        content_topic_name: "CarnotApproval".to_string(),
        encoding: Encoding::Proto,
    });

// TODO: ehm...this should be here, but we will change it whenever the chunking is decided.
const CHUNK_SIZE: usize = 8;

pub struct WakuAdapter {
    network_relay: OutboundRelay<<NetworkService<Waku> as ServiceData>::Message>,
}

impl WakuAdapter {
    async fn message_subscriber_channel(&self) -> Result<Receiver<NetworkEvent>, RecvError> {
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
impl NetworkAdapter for WakuAdapter {
    type Backend = Waku;

    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        Self { network_relay }
    }

    async fn proposal_chunks_stream(&self) -> Box<dyn Stream<Item = BlockChunk>> {
        let stream_channel = self
            .message_subscriber_channel()
            .await
            .unwrap_or_else(|_e| todo!("handle error"));
        Box::new(
            BroadcastStream::new(stream_channel).filter_map(|msg| async move {
                match msg {
                    Ok(event) => match event {
                        NetworkEvent::RawMessage(message) => {
                            // TODO: this should actually check the whole content topic,
                            // waiting for this [PR](https://github.com/waku-org/waku-rust-bindings/pull/28)
                            if WAKU_CARNOT_BLOCK_CONTENT_TOPIC.content_topic_name
                                == message.content_topic().content_topic_name
                            {
                                let payload = message.payload();
                                Some(
                                    ProposalChunkMsg::from_bytes::<CHUNK_SIZE>(
                                        payload.try_into().unwrap(),
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
            }),
        )
    }

    async fn broadcast_block_chunk(&self, _view: View, chunk_message: ProposalChunkMsg) {
        // TODO: probably later, depending on the view we should map to different content topics
        // but this is an ongoing idea that should/will be discus.
        let message = WakuMessage::new::<[u8; CHUNK_SIZE]>(
            chunk_message.as_bytes(),
            WAKU_CARNOT_BLOCK_CONTENT_TOPIC.clone(),
            1,
            chrono::Utc::now().timestamp() as usize,
        );
        if let Err((_, _e)) = self
            .network_relay
            .send(NetworkMsg::Process(WakuBackendMessage::Broadcast {
                message,
                topic: Some(WAKU_CARNOT_PUB_SUB_TOPIC.clone()),
            }))
            .await
        {
            todo!("log error");
        };
    }

    async fn approvals_stream(&self) -> Box<dyn Stream<Item = Approval>> {
        let stream_channel = self
            .message_subscriber_channel()
            .await
            .unwrap_or_else(|_e| todo!("handle error"));
        Box::new(
            BroadcastStream::new(stream_channel).filter_map(|msg| async move {
                match msg {
                    Ok(event) => match event {
                        NetworkEvent::RawMessage(message) => {
                            // TODO: this should actually check the whole content topic,
                            // waiting for this [PR](https://github.com/waku-org/waku-rust-bindings/pull/28)
                            if WAKU_CARNOT_APPROVAL_CONTENT_TOPIC.content_topic_name
                                == message.content_topic().content_topic_name
                            {
                                let payload = message.payload();
                                Some(ApprovalMsg::from_bytes(payload).approval)
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

    async fn forward_approval(&self, approval_message: ApprovalMsg) {
        let message = WakuMessage::new(
            approval_message.as_bytes(),
            WAKU_CARNOT_APPROVAL_CONTENT_TOPIC.clone(),
            1,
            chrono::Utc::now().timestamp() as usize,
        );
        if let Err((_, _e)) = self
            .network_relay
            .send(NetworkMsg::Process(WakuBackendMessage::Broadcast {
                message,
                topic: Some(WAKU_CARNOT_PUB_SUB_TOPIC.clone()),
            }))
            .await
        {
            todo!("log error");
        };
    }
}
