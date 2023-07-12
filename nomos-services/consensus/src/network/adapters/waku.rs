// std
use std::borrow::Cow;
// crates
use futures::{Stream, StreamExt};
use tokio_stream::wrappers::BroadcastStream;
// internal
use crate::network::messages::{NetworkMessage, NewViewMsg, TimeoutMsg, TimeoutQcMsg};
use crate::network::{
    messages::{ProposalChunkMsg, VoteMsg},
    BoxedStream, NetworkAdapter,
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
    ) -> Result<BoxedStream<WakuMessage>, tokio::sync::oneshot::error::RecvError> {
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

    async fn inner_broadcast(&self, payload: Box<[u8]>, content_topic: WakuContentTopic) {
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

#[async_trait::async_trait]
impl NetworkAdapter for WakuAdapter {
    type Backend = Waku;

    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        Self { network_relay }
    }

    async fn proposal_chunks_stream(&self, view: View) -> BoxedStream<ProposalChunkMsg> {
        Box::new(Box::pin(
            self.cached_stream_with_content_topic(create_topic(PROPOSAL_TAG, None))
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

    async fn broadcast(&self, message: NetworkMessage) {
        let topic = create_topic(message_tag(&message), None);
        self.inner_broadcast(unwrap_message_to_bytes(&message), topic)
            .await
    }

    async fn timeout_stream(&self, committee: &Committee, view: View) -> BoxedStream<TimeoutMsg> {
        let content_topic = create_topic(TIMEOUT_TAG, Some(committee));
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

    async fn timeout_qc_stream(&self, view: View) -> BoxedStream<TimeoutQcMsg> {
        Box::new(Box::pin(
            self.cached_stream_with_content_topic(create_topic(TIMEOUT_QC_TAG, None))
                .await
                .filter_map(move |message| {
                    let payload = message.payload();
                    let qc = TimeoutQcMsg::from_bytes(payload);
                    async move {
                        if qc.qc.view() == view {
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
    ) -> BoxedStream<VoteMsg> {
        let content_topic = create_topic(VOTE_TAG, Some(committee));
        Box::new(Box::pin(
            self.cached_stream_with_content_topic(content_topic)
                .await
                .filter_map(move |message| {
                    let payload = message.payload();
                    let vote = VoteMsg::from_bytes(payload);
                    async move {
                        if vote.vote.block == proposal_id && vote.vote.view == view {
                            Some(vote)
                        } else {
                            None
                        }
                    }
                }),
        ))
    }

    async fn new_view_stream(&self, committee: &Committee, view: View) -> BoxedStream<NewViewMsg> {
        let content_topic = create_topic(NEW_VIEW_TAG, Some(committee));
        Box::new(Box::pin(
            self.cached_stream_with_content_topic(content_topic)
                .await
                .filter_map(move |message| {
                    let payload = message.payload();
                    let new_view = NewViewMsg::from_bytes(payload);
                    async move {
                        if new_view.vote.view == view {
                            Some(new_view)
                        } else {
                            None
                        }
                    }
                }),
        ))
    }

    async fn send(&self, message: NetworkMessage, committee: &Committee) {
        let topic = create_topic(message_tag(&message), Some(committee));
        self.inner_broadcast(unwrap_message_to_bytes(&message), topic)
            .await
    }
}

fn create_topic(tag: &str, committee: Option<&Committee>) -> WakuContentTopic {
    WakuContentTopic {
        application_name: Cow::Borrowed(APPLICATION_NAME),
        version: VERSION,
        content_topic_name: Cow::Owned(format!(
            "{}{}",
            tag,
            committee
                .map(|c| format!("-{}", c.id::<blake2::Blake2s256>()))
                .unwrap_or_default()
        )),
        encoding: Encoding::Proto,
    }
}

// since we use content topic to filter messages, we can remove the tag from the message
fn unwrap_message_to_bytes(message: &NetworkMessage) -> Box<[u8]> {
    match message {
        NetworkMessage::NewView(msg) => msg.as_bytes(),
        NetworkMessage::ProposalChunk(msg) => msg.as_bytes(),
        NetworkMessage::Vote(msg) => msg.as_bytes(),
        NetworkMessage::Timeout(msg) => msg.as_bytes(),
        NetworkMessage::TimeoutQc(msg) => msg.as_bytes(),
    }
}

fn message_tag(message: &NetworkMessage) -> &str {
    match message {
        NetworkMessage::NewView(_) => NEW_VIEW_TAG,
        NetworkMessage::ProposalChunk(_) => PROPOSAL_TAG,
        NetworkMessage::Vote(_) => VOTE_TAG,
        NetworkMessage::Timeout(_) => TIMEOUT_TAG,
        NetworkMessage::TimeoutQc(_) => TIMEOUT_QC_TAG,
    }
}

const NEW_VIEW_TAG: &str = "new-view";
const PROPOSAL_TAG: &str = "proposal";
const VOTE_TAG: &str = "vote";
const TIMEOUT_TAG: &str = "timeout";
const TIMEOUT_QC_TAG: &str = "timeout-qc";
