// std
use std::borrow::Cow;
// crates
use bytes::Bytes;
use futures::{Stream, StreamExt};
use tokio_stream::wrappers::BroadcastStream;
// internal
use crate::network::{
    messages::{ApprovalMsg, ProposalChunkMsg},
    NetworkAdapter,
};
use crate::overlay::committees::Committee;
use crate::{Approval, View};
use nomos_network::{
    backends::waku::{EventKind, NetworkEvent, Waku, WakuBackendMessage},
    NetworkMsg, NetworkService,
};
use overwatch_rs::services::{relay::OutboundRelay, ServiceData};
use waku_bindings::{Encoding, WakuContentTopic, WakuMessage, WakuPubSubTopic};

const WAKU_CARNOT_PUB_SUB_TOPIC: WakuPubSubTopic =
    WakuPubSubTopic::new("CarnotSim", Encoding::Proto);

const APPLICATION_NAME: &str = "CarnotSim";
const VERSION: usize = 1;

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

    async fn proposal_chunks_stream(
        &self,
        committee: Committee,
        view: &View,
    ) -> Box<dyn Stream<Item = Bytes> + Send + Sync + Unpin> {
        let stream_channel = self
            .message_subscriber_channel()
            .await
            .unwrap_or_else(|_e| todo!("handle error"));
        let content_topic = proposal_topic(committee, view);
        Box::new(
            BroadcastStream::new(stream_channel)
                .zip(futures::stream::repeat(content_topic))
                .filter_map(|(msg, content_topic)| {
                    Box::pin(async move {
                        match msg {
                            Ok(event) => match event {
                                NetworkEvent::RawMessage(message) => {
                                    if &content_topic == message.content_topic() {
                                        let payload = message.payload();
                                        Some(ProposalChunkMsg::from_bytes(payload).chunk)
                                    } else {
                                        None
                                    }
                                }
                            },
                            Err(_e) => None,
                        }
                    })
                }),
        )
    }

    async fn broadcast_block_chunk(
        &self,
        committee: Committee,
        view: &View,
        chunk_message: ProposalChunkMsg,
    ) {
        let content_topic = proposal_topic(committee, view);

        let message = WakuMessage::new(
            chunk_message.as_bytes(),
            content_topic,
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

    async fn approvals_stream(
        &self,
        committee: Committee,
        view: &View,
    ) -> Box<dyn Stream<Item = Approval> + Send> {
        let content_topic = approval_topic(committee, view);
        let stream_channel = self
            .message_subscriber_channel()
            .await
            .unwrap_or_else(|_e| todo!("handle error"));
        Box::new(
            BroadcastStream::new(stream_channel)
                .zip(futures::stream::repeat(content_topic))
                .filter_map(|(msg, content_topic)| async move {
                    match msg {
                        Ok(event) => match event {
                            NetworkEvent::RawMessage(message) => {
                                if &content_topic == message.content_topic() {
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

    async fn forward_approval(
        &self,
        committee: Committee,
        view: &View,
        approval_message: ApprovalMsg,
    ) {
        let content_topic = approval_topic(committee, view);

        let message = WakuMessage::new(
            approval_message.as_bytes(),
            content_topic,
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

fn approval_topic(committee: Committee, view: &View) -> WakuContentTopic {
    WakuContentTopic {
        application_name: Cow::Borrowed(APPLICATION_NAME),
        version: VERSION,
        content_topic_name: Cow::Owned(format!("approval-{}-{}", committee.id(), view.id())),
        encoding: Encoding::Proto,
    }
}

fn proposal_topic(committee: Committee, view: &View) -> WakuContentTopic {
    WakuContentTopic {
        application_name: Cow::Borrowed(APPLICATION_NAME),
        version: VERSION,
        content_topic_name: Cow::Owned(format!("proposal-{}-{}", committee.id(), view.id())),
        encoding: Encoding::Proto,
    }
}
