// std
use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
// crates
use bytes::Bytes;
use futures::{Stream, StreamExt};
use tokio_stream::wrappers::BroadcastStream;
// internal
use crate::network::messages::{TimeoutMsg, TimeoutQcMsg};
use crate::network::{
    messages::{ProposalChunkMsg, VoteMsg},
    NetworkAdapter,
};
use consensus_engine::{Committee, TimeoutQc, View};
use nomos_network::{
    backends::waku::{EventKind, NetworkEvent, Waku, WakuBackendMessage},
    NetworkMsg, NetworkService,
};
use overwatch_rs::services::{relay::OutboundRelay, ServiceData};
use serde::de::DeserializeOwned;
use serde::Serialize;
use waku_bindings::{
    ContentFilter, Encoding, StoreQuery, WakuContentTopic, WakuMessage, WakuPubSubTopic,
};

pub const WAKU_CARNOT_PUB_SUB_TOPIC: WakuPubSubTopic =
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

    async fn archive_subscriber_stream(
        &self,
        content_topic: WakuContentTopic,
    ) -> Result<
        Box<dyn Stream<Item = WakuMessage> + Send + Sync + Unpin>,
        tokio::sync::oneshot::error::RecvError,
    > {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        if let Err((_, _e)) = self
            .network_relay
            .send(NetworkMsg::Process(WakuBackendMessage::ArchiveSubscribe {
                query: StoreQuery {
                    pubsub_topic: Some(WAKU_CARNOT_PUB_SUB_TOPIC.clone()),
                    content_filters: vec![ContentFilter::new(content_topic)],
                    // TODO: maybe handle limits through configuration
                    start_time: None,
                    end_time: None,
                    paging_options: None,
                },
                reply_channel: sender,
            }))
            .await
        {
            todo!("log error");
        };
        receiver.await
    }

    async fn cached_stream_with_content_topic(
        &self,
        content_topic: WakuContentTopic,
    ) -> impl Stream<Item = WakuMessage> {
        // create stream request tasks
        let live_stream_channel_task = self.message_subscriber_channel();
        let archive_stream_task = self.archive_subscriber_stream(content_topic.clone());
        // wait for both tasks to complete
        let (live_stream_channel_result, archive_stream_result) =
            futures::join!(live_stream_channel_task, archive_stream_task);
        // unwrap results
        let live_stream_channel =
            live_stream_channel_result.expect("live stream channel from waku network");
        let archive_stream = archive_stream_result.expect("archive stream from waku network");
        let live_stream = BroadcastStream::new(live_stream_channel)
            .zip(futures::stream::repeat(content_topic))
            .filter_map(|(msg, content_topic)| async move {
                match msg {
                    Ok(NetworkEvent::RawMessage(message))
                        if message.content_topic() == &content_topic =>
                    {
                        Some(message)
                    }
                    _ => None,
                }
            });
        tokio_stream::StreamExt::merge(archive_stream, live_stream)
    }

    async fn broadcast(&self, bytes: Box<[u8]>, topic: WakuContentTopic) {
        let message = WakuMessage::new(bytes, topic, 1, chrono::Utc::now().timestamp() as usize);
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
        view: View,
    ) -> Box<dyn Stream<Item = Bytes> + Send + Sync + Unpin> {
        Box::new(Box::pin(
            self.cached_stream_with_content_topic(PROPOSAL_CONTENT_TOPIC.clone())
                .await
                .filter_map(move |message| {
                    let payload = message.payload();
                    let ProposalChunkMsg {
                        view: msg_view,
                        chunk,
                    } = ProposalChunkMsg::from_bytes(payload);
                    async move {
                        if view == msg_view {
                            Some(Bytes::from(chunk))
                        } else {
                            None
                        }
                    }
                }),
        ))
    }

    async fn broadcast_block_chunk(&self, chunk_message: ProposalChunkMsg) {
        self.broadcast(chunk_message.as_bytes(), PROPOSAL_CONTENT_TOPIC)
            .await
    }

    async fn broadcast_timeout_qc(&self, timeout_qc_msg: TimeoutQcMsg) {
        self.broadcast(timeout_qc_msg.as_bytes(), TIMEOUT_QC_CONTENT_TOPIC)
            .await
    }

    async fn timeout_stream(
        &self,
        committee: &Committee,
        view: View,
    ) -> Box<dyn Stream<Item = TimeoutMsg> + Send + Sync + Unpin> {
        let content_topic = create_topic("timeout", committee, view);
        Box::new(Box::pin(
            self.cached_stream_with_content_topic(content_topic)
                .await
                .filter_map(move |message| {
                    let payload = message.payload();
                    let timeout = TimeoutMsg::from_bytes(payload);
                    async move {
                        if timeout.vote.view == view {
                            Some(timeout)
                        } else {
                            None
                        }
                    }
                }),
        ))
    }

    async fn timeout_qc_stream(
        &self,
        view: View,
    ) -> Box<dyn Stream<Item = TimeoutQc> + Send + Sync + Unpin> {
        Box::new(Box::pin(
            self.cached_stream_with_content_topic(TIMEOUT_QC_CONTENT_TOPIC.clone())
                .await
                .filter_map(move |message| {
                    let payload = message.payload();
                    let qc = TimeoutQcMsg::from_bytes(payload).qc;
                    async move {
                        if qc.view > view {
                            Some(qc)
                        } else {
                            None
                        }
                    }
                }),
        ))
    }

    async fn votes_stream<Vote: DeserializeOwned>(
        &self,
        committee: &Committee,
        view: View,
    ) -> Box<dyn Stream<Item = VoteMsg> + Send> {
        let content_topic = create_topic("votes", committee, view);
        Box::new(Box::pin(
            self.cached_stream_with_content_topic(content_topic)
                .await
                .map(|message| {
                    let payload = message.payload();
                    VoteMsg::from_bytes(payload)
                }),
        ))
    }

    async fn send_vote<Vote: Serialize + Send>(
        &self,
        committee: &Committee,
        view: View,
        approval_message: VoteMsg,
    ) {
        let content_topic = create_topic("votes", committee, view);

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

fn create_topic(tag: &str, committee: &Committee, view: View) -> WakuContentTopic {
    WakuContentTopic {
        application_name: Cow::Borrowed(APPLICATION_NAME),
        version: VERSION,
        content_topic_name: Cow::Owned(format!("{}-{}-{}", tag, hash_set(committee), view)),
        encoding: Encoding::Proto,
    }
}

const PROPOSAL_CONTENT_TOPIC: WakuContentTopic =
    WakuContentTopic::new(APPLICATION_NAME, VERSION, "proposal", Encoding::Proto);
const TIMEOUT_QC_CONTENT_TOPIC: WakuContentTopic =
    WakuContentTopic::new(APPLICATION_NAME, VERSION, "timeout-qc", Encoding::Proto);

// TODO: Maybe use a secure hasher instead
fn hash_set(c: &Committee) -> u64 {
    let mut s = DefaultHasher::new();
    for e in c.iter() {
        e.hash(&mut s);
    }
    s.finish()
}
