// std
// crates
use futures::{Stream, StreamExt};
use tokio_stream::wrappers::BroadcastStream;
// internal
use crate::network::{
    messages::{ApprovalMsg, ProposalChunkMsg},
    NetworkAdapter,
};
use crate::{Approval, View};
use nomos_core::block::BlockChunk;
use nomos_network::{
    backends::waku::{EventKind, NetworkEvent, Waku, WakuBackendMessage},
    NetworkMsg, NetworkService,
};
use overwatch_rs::services::{relay::OutboundRelay, ServiceData};
use waku_bindings::{Encoding, WakuContentTopic, WakuMessage, WakuPubSubTopic};

static WAKU_CARNOT_PUB_SUB_TOPIC: WakuPubSubTopic =
    WakuPubSubTopic::new("CarnotSim", Encoding::Proto);

static WAKU_CARNOT_BLOCK_CONTENT_TOPIC: WakuContentTopic =
    WakuContentTopic::new("CarnotSim", 1, "CarnotBlock", Encoding::Proto);

static WAKU_CARNOT_APPROVAL_CONTENT_TOPIC: WakuContentTopic =
    WakuContentTopic::new("CarnotSim", 1, "CarnotApproval", Encoding::Proto);

// TODO: ehm...this should be here, but we will change it whenever the chunking is decided.
const CHUNK_SIZE: usize = 8;

pub struct WakuAdapter {
    network_relay: OutboundRelay<<NetworkService<Waku> as ServiceData>::Message>,
}

impl WakuAdapter {
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
