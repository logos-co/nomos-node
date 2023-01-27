// internal
use super::*;

// crates
use futures::{select, FutureExt};
use overwatch_rs::services::state::NoState;
use rand::{distributions::{WeightedIndex, Distribution}, rngs::StdRng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
use tokio::sync::{
    broadcast::{self, Receiver, Sender},
    mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
};
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

/// A mock pubsub topic in the form of `/mock/{topic_name}/`
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
    subscribed_topics: HashSet<&'static str>,
    subscribe_tx: UnboundedSender<&'static str>,
    subscribe_rx: UnboundedReceiver<&'static str>,
    unsubscribe_tx: UnboundedSender<&'static str>,
    unsubscribe_rx: UnboundedReceiver<&'static str>,
    config: MockConfig<D>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MockConfig<D = rand::distributions::Standard> {
    pub predefined_messages: Vec<MockMessage>,
    pub duration: std::time::Duration,
    pub distributions: D,
    pub seed: u64,
    pub version: usize,
    pub weights: Option<Vec<usize>>,
}

/// Interaction with Mock node
#[derive(Debug)]
pub enum MockBackendMessage {
    Broadcast {
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
    /// Run producer message handler
    pub async fn run_producer_handler(&self) -> Result<(), overwatch_rs::DynError> {
        match &self.config.weights {
            // if user provides weights, then we send the predefined messages according to the weights endlessly
            Some(weights) => {
                let dist = WeightedIndex::new(weights.iter())?;
                let mut rng = StdRng::seed_from_u64(self.config.seed);
                loop {
                    let idx = dist.sample(&mut rng);
                    tokio::time::sleep(self.config.duration).await;
                    match self.message_event.send(NetworkEvent::RawMessage(
                        self.config.predefined_messages[idx].clone(),
                    )) {
                        Ok(peers) => {
                            tracing::debug!("sent message to {} peers", peers);
                        }
                        Err(e) => {
                            tracing::error!("error sending message: {:?}", e);
                        }
                    };
                }
            },
            // if user do not provide weights, then we just send the predefined messages one by one in order
            None => {
                for msg in &self.config.predefined_messages {
                    tokio::time::sleep(self.config.duration).await;
                    match self.message_event.send(NetworkEvent::RawMessage(msg.clone())) {
                        Ok(peers) => {
                            tracing::debug!("sent message to {} peers", peers);
                        }
                        Err(e) => {
                            tracing::error!("error sending message: {:?}", e);
                        }
                    };
                }
            },
        }
        Ok(())
    }

    /// Run subscriber message handler.
    pub async fn run_subscribe_handler(&mut self) {
        loop {
            select! {
                topic = self.subscribe_rx.recv().fuse() => {
                    match topic {
                        Some(topic) => {
                            tracing::debug!("subscribed to topic: {}", topic);
                            self.subscribed_topics.insert(topic);
                        },
                        None => {
                            tracing::error!("error subscribing to topic");
                        }
                    }
                }
                topic = self.unsubscribe_rx.recv().fuse() => {
                    match topic {
                        Some(topic) => {
                            tracing::debug!("unsubscribed from topic: {}", topic);
                            self.subscribed_topics.remove(topic);
                        }
                        None => {
                            tracing::error!("error unsubscribing from topic");
                        }
                    }
                }
                default => {}
            }
        }
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

        let (subscribe_tx, subscribe_rx) = unbounded_channel();
        let (unsubscribe_tx, unsubscribe_rx) = unbounded_channel();

        Self {
            subscribed_topics: HashSet::new(),
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
            MockBackendMessage::Broadcast { topic, msg } => {
                debug!("processed normal message");
                let mut normal_msgs = self.messages.lock().unwrap();
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
                match self.subscribe_tx.send(topic) {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!("failed to send subscription: {e}");
                    }
                }
            }
            MockBackendMessage::RelayUnSubscribe { topic } => {
                debug!("processed relay unsubscription for topic: {topic}");
                match self.unsubscribe_tx.send(topic) {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!("failed to send subscription: {e}");
                    }
                }
            }
            MockBackendMessage::Query { topic, tx } => {
                debug!("processed query");
                let normal_msgs = self.messages.lock().unwrap();
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
