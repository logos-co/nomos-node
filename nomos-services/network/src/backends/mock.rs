use super::*;
use futures::{select, FutureExt};
use overwatch_rs::services::state::NoState;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::broadcast::{self, Receiver, Sender};
use tracing::debug;

const BROADCAST_CHANNEL_BUF: usize = 16;

pub type MockMessageVersion = usize;

/// A mock content topic `/{application_name}/{version}/{content_topic_name}`
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MockContentTopic {
    pub application_name: &'static str,
    pub version: usize,
    pub content_topic_name: &'static str,
}

impl MockContentTopic {
    pub const fn new(
        application_name: &'static str,
        version: usize,
        content_topic_name: &'static str,
    ) -> Self {
        Self {
            application_name,
            version,
            content_topic_name,
        }
    }
}

/// A mock pubsub topic in the form of `/mock/v2/{topic_name}/{encoding}`
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MockPubSubTopic {
    pub topic_name: &'static str,
}

impl MockPubSubTopic {
    pub const fn new(topic_name: &'static str) -> Self {
        Self { topic_name }
    }
}

#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MockMessage {
    payload: String,
    /// The content topic to be set on the message
    content_topic: MockContentTopic,
    /// The Mock Message version number
    #[serde(default)]
    version: MockMessageVersion,
    /// Unix timestamp in nanoseconds
    timestamp: usize,
}

impl MockMessage {
    pub const fn content_topic(&self) -> MockContentTopic {
        self.content_topic
    }

    pub fn payload(&self) -> String {
        self.payload.clone()
    }
}

pub struct Mock<D: rand::distributions::Distribution<usize>> {
    messages: Arc<Mutex<HashMap<&'static str, Vec<String>>>>,
    message_event: Sender<NetworkEvent>,
    subscribe_tx: async_channel::Sender<&'static str>,
    subscribe_rx: async_channel::Receiver<&'static str>,
    unsubscribe_tx: async_channel::Sender<&'static str>,
    unsubscribe_rx: async_channel::Receiver<&'static str>,
    config: MockConfig<D>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MockConfig<D = rand::distributions::Standard> {
    pub predefined_messages: Vec<MockMessage>,
    pub duration: std::time::Duration,
    pub distributions: D,
    pub seed: u64,
    pub version: usize,
}

/// Interaction with Mock node
#[derive(Debug)]
pub enum MockBackendMessage {
    Normal {
        topic: &'static str,
        msg: String,
    },
    RelaySubscribe {
        topic: &'static str,
    },
    RelayUnSubscribe {
        topic: &'static str,
    },
    Query {
        topic: &'static str,
        tx: oneshot::Sender<Vec<String>>,
    },
}

#[derive(Debug)]
pub enum EventKind {
    Message,
}

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    RawMessage(MockMessage),
}

impl<D: rand::distributions::Distribution<usize> + Clone + Debug + Send + Sync + 'static> Mock<D> {
    pub fn producer(&self) -> MockProducer<D> {
        MockProducer::<D>::new(
            self.message_event.clone(),
            self.config.predefined_messages.clone(),
            self.config.duration,
            self.config.distributions.clone(),
            self.config.seed,
        )
    }

    pub fn subscriber(&self) -> MockSubscriber {
        MockSubscriber::new(self.subscribe_rx.clone(), self.unsubscribe_rx.clone())
    }
}

#[async_trait::async_trait]
impl<D: rand::distributions::Distribution<usize> + Debug + Clone + Send + Sync + 'static>
    NetworkBackend for Mock<D>
{
    type Settings = MockConfig<D>;
    type State = NoState<MockConfig<D>>;
    type Message = MockBackendMessage;
    type EventKind = EventKind;
    type NetworkEvent = NetworkEvent;

    fn new(config: Self::Settings) -> Self {
        let message_event = broadcast::channel(BROADCAST_CHANNEL_BUF).0;

        let (subscribe_tx, subscribe_rx) = async_channel::unbounded();
        let (unsubscribe_tx, unsubscribe_rx) = async_channel::unbounded();

        Self {
            subscribe_tx,
            subscribe_rx,
            unsubscribe_tx,
            unsubscribe_rx,
            messages: Arc::new(Mutex::new(
                config
                    .predefined_messages
                    .iter()
                    .map(|p| (p.content_topic.content_topic_name, Vec::new()))
                    .collect(),
            )),
            message_event,
            config,
        }
    }

    async fn process(&self, msg: Self::Message) {
        match msg {
            MockBackendMessage::Normal { topic, msg } => {
                debug!("processed normal message");
                let mut normal_msgs = self.messages.lock();
                normal_msgs
                    .entry(topic)
                    .or_insert_with(Vec::new)
                    .push(msg.clone());
                drop(normal_msgs);
                let _ = self
                    .message_event
                    .send(NetworkEvent::RawMessage(MockMessage {
                        payload: msg,
                        content_topic: MockContentTopic::new("mock", self.config.version, topic),
                        version: self.config.version,
                        timestamp: chrono::Utc::now().timestamp() as usize,
                    }));
            }
            MockBackendMessage::RelaySubscribe { topic } => {
                debug!("processed relay subscription for topic: {topic}");
                match self.subscribe_tx.send(topic).await {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!("failed to send subscription: {e}");
                    }
                }
            }
            MockBackendMessage::RelayUnSubscribe { topic } => {
                debug!("processed relay unsubscription for topic: {topic}");
                match self.unsubscribe_tx.send(topic).await {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!("failed to send subscription: {e}");
                    }
                }
            }
            MockBackendMessage::Query { topic, tx } => {
                debug!("processed query");
                let normal_msgs = self.messages.lock();
                let msgs = normal_msgs.get(&topic).cloned().unwrap_or_default();
                drop(normal_msgs);
                let _ = tx.send(msgs);
            }
        };
    }

    async fn subscribe(&mut self, kind: Self::EventKind) -> Receiver<Self::NetworkEvent> {
        match kind {
            EventKind::Message => {
                debug!("processed subscription to incoming messages");
                self.message_event.subscribe()
            }
        }
    }
}

/// handle mock messages producer
pub struct MockProducer<D: rand::distributions::Distribution<usize> = rand::distributions::Standard>
{
    distributions: D,
    predefined_messages: Vec<MockMessage>,
    duration: std::time::Duration,
    tx: Sender<NetworkEvent>,
    seed: u64,
}

impl<D: rand::distributions::Distribution<usize>> MockProducer<D> {
    pub fn new(
        tx: Sender<NetworkEvent>,
        predefined_messages: Vec<MockMessage>,
        duration: std::time::Duration,
        distributions: D,
        seed: u64,
    ) -> Self {
        Self {
            predefined_messages,
            duration,
            tx,
            distributions,
            seed,
        }
    }

    pub async fn run(self) {
        use rand::SeedableRng;
        let messages = self
            .distributions
            .sample_iter(rand::rngs::StdRng::seed_from_u64(self.seed))
            .take(self.predefined_messages.len())
            .map(|idx| self.predefined_messages[idx].clone())
            .collect::<Vec<_>>();

        for msg in messages {
            tokio::time::sleep(self.duration).await;
            match self.tx.clone().send(NetworkEvent::RawMessage(msg)) {
                Ok(peers) => {
                    tracing::debug!("sent message to {} peers", peers);
                }
                Err(e) => {
                    tracing::error!("error sending message: {:?}", e);
                }
            };
        }
    }
}

/// handle mock subscribe messages
pub struct MockSubscriber {
    subscribed_topics: HashMap<&'static str, usize>,
    subscribe_rx: async_channel::Receiver<&'static str>,
    unsubscribe_rx: async_channel::Receiver<&'static str>,
}

impl MockSubscriber {
    pub fn new(
        subscribe_rx: async_channel::Receiver<&'static str>,
        unsubscribe_rx: async_channel::Receiver<&'static str>,
    ) -> Self {
        Self {
            subscribed_topics: Default::default(),
            subscribe_rx,
            unsubscribe_rx,
        }
    }

    pub async fn run(mut self) {
        loop {
            select! {
                topic = self.subscribe_rx.recv().fuse() => {
                    match topic {
                        Ok(topic) => {
                            tracing::debug!("subscribed to topic: {}", topic);
                            self.subscribed_topics.entry(topic).and_modify(|v| *v += 1).or_insert(1);
                        },
                        Err(e) => {
                            tracing::error!("error subscribing to topic: {:?}", e);
                        }
                    }
                }
                topic = self.unsubscribe_rx.recv().fuse() => {
                    match topic {
                        Ok(topic) => {
                            tracing::debug!("unsubscribed from topic: {}", topic);
                            self.subscribed_topics.entry(topic).and_modify(|v| {
                                if *v == 0 {
                                    panic!("unsubscribed from topic {} more times than subscribed", topic);
                                } else {
                                    *v -= 1;
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!("error unsubscribing from topic: {:?}", e);
                        }
                    }
                }
                default => {}
            }
        }
    }
}
