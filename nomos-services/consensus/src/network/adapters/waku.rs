// std
use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
// crates
use futures::{Stream, StreamExt};
use tokio_stream::wrappers::BroadcastStream;
// internal
use crate::network::messages::{NewViewMsg, TimeoutMsg, TimeoutQcMsg};
use crate::network::{
    messages::{ProposalChunkMsg, VoteMsg},
    NetworkAdapter,
};
use consensus_engine::{BlockId, Committee, View};
use nomos_network::{
    backends::waku::{EventKind, NetworkEvent, Waku, WakuBackendMessage},
    NetworkMsg, NetworkService,
};
use overwatch_rs::services::{relay::OutboundRelay, ServiceData};
use waku_bindings::{
    ContentFilter, Encoding, StoreQuery, WakuContentTopic, WakuMessage, WakuPubSubTopic,
};

pub const WAKU_CARNOT_PUB_SUB_TOPIC: WakuPubSubTopic =
    WakuPubSubTopic::new("CarnotSim", Encoding::Proto);

const APPLICATION_NAME: &str = "CarnotSim";
const VERSION: usize = 1;

#[derive(Clone)]
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
        network_relay: OutboundRelay<<NetworkService<Waku> as ServiceData>::Message>,
        content_topic: WakuContentTopic,
    ) -> Result<
        Box<dyn Stream<Item = WakuMessage> + Send + Sync + Unpin>,
        tokio::sync::oneshot::error::RecvError,
    > {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        if let Err((_, _e)) = network_relay
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
        let live_stream_channel = self
            .message_subscriber_channel()
            .await
            .expect("live stream channel from waku network");

        struct InnerState {
            first: bool,
            topic: WakuContentTopic,
            relay: OutboundRelay<<NetworkService<Waku> as ServiceData>::Message>,
        }

        let state = InnerState {
            first: true,
            topic: content_topic.clone(),
            relay: self.network_relay.clone(),
        };

        // Sometimes waku takes a while make a message available in the archive, so we keep polling the archive until we get the message we want.
        // This stream will generate a new archive stream every 100ms until the message is found, chaining them together.
        // We expect this to be a rare occurrence, normal operation would result in this future being discarded before we even try to make a second request.
        let archive_stream = futures::stream::unfold(state, |mut state| async move {
            if !state.first {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
            state.first = false;
            Some((
                Self::archive_subscriber_stream(state.relay.clone(), state.topic.clone())
                    .await
                    .expect("archive stream from waku network"),
                state,
            ))
        })
        .flatten();

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
        tokio_stream::StreamExt::merge(live_stream, archive_stream)
    }

    async fn broadcast(&self, bytes: Box<[u8]>, topic: WakuContentTopic) {
        let message = WakuMessage::new(
            bytes,
            topic,
            1,
            chrono::Utc::now().timestamp_nanos() as usize,
            [],
            false,
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
    ) -> Box<dyn Stream<Item = ProposalChunkMsg> + Send + Sync + Unpin> {
        Box::new(Box::pin(
            self.cached_stream_with_content_topic(PROPOSAL_CONTENT_TOPIC)
                .await
                .filter_map(move |message| {
                    let payload = message.payload();
                    let proposal = ProposalChunkMsg::from_bytes(payload);
                    async move {
                        if view == proposal.view {
                            Some(proposal)
                        } else {
                            None
                        }
                    }
                }),
        ))
    }

    async fn broadcast_block_chunk(&self, chunk_message: ProposalChunkMsg) {
        let message = WakuMessage::new(
            chunk_message.as_bytes(),
            PROPOSAL_CONTENT_TOPIC,
            1,
            chrono::Utc::now().timestamp_nanos() as usize,
            [],
            false,
        );
        if let Err((_, _e)) = self
            .network_relay
            .send(NetworkMsg::Process(WakuBackendMessage::Broadcast {
                message,
                topic: Some(WAKU_CARNOT_PUB_SUB_TOPIC),
            }))
            .await
        {
            todo!("log error");
        };
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
    ) -> Box<dyn Stream<Item = TimeoutQcMsg> + Send + Sync + Unpin> {
        Box::new(Box::pin(
            self.cached_stream_with_content_topic(TIMEOUT_QC_CONTENT_TOPIC)
                .await
                .filter_map(move |message| {
                    let payload = message.payload();
                    let qc = TimeoutQcMsg::from_bytes(payload);
                    async move {
                        if qc.qc.view > view {
                            Some(qc)
                        } else {
                            None
                        }
                    }
                }),
        ))
    }

    async fn votes_stream(
        &self,
        committee: &Committee,
        view: View,
        proposal_id: BlockId,
    ) -> Box<dyn Stream<Item = VoteMsg> + Send + Unpin> {
        let content_topic = create_topic("votes", committee, view);
        Box::new(Box::pin(
            self.cached_stream_with_content_topic(content_topic)
                .await
                .filter_map(move |message| {
                    let payload = message.payload();
                    let vote = VoteMsg::from_bytes(payload);
                    async move {
                        if vote.vote.block == proposal_id {
                            Some(vote)
                        } else {
                            None
                        }
                    }
                }),
        ))
    }

    async fn new_view_stream(
        &self,
        committee: &Committee,
        view: View,
    ) -> Box<dyn Stream<Item = NewViewMsg> + Send + Unpin> {
        let content_topic = create_topic("votes", committee, view);
        Box::new(Box::pin(
            self.cached_stream_with_content_topic(content_topic)
                .await
                .map(|message| {
                    let payload = message.payload();
                    NewViewMsg::from_bytes(payload)
                }),
        ))
    }

    async fn send(&self, committee: &Committee, view: View, payload: Box<[u8]>, channel: &str) {
        let content_topic = create_topic(channel, committee, view);

        let message = WakuMessage::new(
            payload,
            content_topic,
            1,
            chrono::Utc::now().timestamp_nanos() as usize,
            [],
            false,
        );
        if let Err((_, e)) = self
            .network_relay
            .send(NetworkMsg::Process(WakuBackendMessage::Broadcast {
                message,
                topic: Some(WAKU_CARNOT_PUB_SUB_TOPIC),
            }))
            .await
        {
            tracing::error!("waku message send error: {e:?}");
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
