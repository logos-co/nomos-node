// internal
use super::*;

use futures::future::BoxFuture;
// crates
use overwatch_rs::services::state::NoState;
use rand::{
    distributions::{Distribution, WeightedIndex},
    rngs::StdRng,
    SeedableRng,
};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast::{self, Receiver, Sender};
use tracing::debug;

const BROADCAST_CHANNEL_BUF: usize = 16;

pub type MockMessageVersion = usize;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct MockContentTopic {
    pub application_name: Cow<'static, str>,
    pub version: usize,
    pub content_topic_name: Cow<'static, str>,
}

impl MockContentTopic {
    pub const fn new(
        application_name: &'static str,
        version: usize,
        content_topic_name: &'static str,
    ) -> Self {
        Self {
            application_name: Cow::Borrowed(application_name),
            version,
            content_topic_name: Cow::Borrowed(content_topic_name),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MockPubSubTopic {
    pub topic_name: Cow<'static, str>,
}

impl MockPubSubTopic {
    pub const fn new(topic_name: &'static str) -> Self {
        Self {
            topic_name: Cow::Borrowed(topic_name),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MockMessage {
    pub payload: String,
    /// The content topic to be set on the message
    pub content_topic: MockContentTopic,
    /// The Mock Message version number
    #[serde(default)]
    pub version: MockMessageVersion,
    /// Unix timestamp in nanoseconds
    pub timestamp: usize,
}

impl MockMessage {
    pub const fn new(
        payload: String,
        content_topic: MockContentTopic,
        version: MockMessageVersion,
        timestamp: usize,
    ) -> Self {
        Self {
            payload,
            content_topic,
            version,
            timestamp,
        }
    }

    pub const fn content_topic(&self) -> &MockContentTopic {
        &self.content_topic
    }

    pub fn payload(&self) -> String {
        self.payload.clone()
    }
}

#[derive(Clone)]
pub struct Mock {
    messages: Arc<Mutex<HashMap<String, Vec<MockMessage>>>>,
    message_event: Sender<NetworkEvent>,
    subscribed_topics: Arc<Mutex<HashSet<String>>>,
    config: MockConfig,
}

#[derive(Clone, Debug)]
pub struct MockConfig {
    pub predefined_messages: Vec<MockMessage>,
    pub duration: std::time::Duration,
    pub seed: u64,
    pub version: usize,
    pub weights: Option<Vec<usize>>,
}

pub enum MockBackendMessage {
    BootProducer {
        #[allow(clippy::type_complexity)]
        spawner: Box<
            dyn Fn(
                    BoxFuture<'static, Result<(), overwatch_rs::DynError>>,
                ) -> Result<(), overwatch_rs::DynError>
                + Send
                + Sync
                + 'static,
        >,
    },
    Broadcast {
        topic: String,
        msg: MockMessage,
    },
    RelaySubscribe {
        topic: String,
    },
    RelayUnSubscribe {
        topic: String,
    },
    Query {
        topic: String,
        tx: oneshot::Sender<Vec<MockMessage>>,
    },
}

impl core::fmt::Debug for MockBackendMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BootProducer { .. } => write!(f, "BootProducer"),
            Self::Broadcast { topic, msg } => {
                write!(f, "Broadcast {{ topic: {topic}, msg: {msg:?} }}")
            }
            Self::RelaySubscribe { topic } => write!(f, "RelaySubscribe {{ topic: {topic} }}"),
            Self::RelayUnSubscribe { topic } => {
                write!(f, "RelayUnSubscribe {{ topic: {topic} }}")
            }
            Self::Query { topic, .. } => write!(f, "Query {{ topic: {topic} }}"),
        }
    }
}

#[derive(Debug)]
pub enum EventKind {
    Message,
}

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    RawMessage(MockMessage),
}

impl Mock {
    /// Run producer message handler
    pub async fn run_producer_handler(&self) -> Result<(), overwatch_rs::DynError> {
        match &self.config.weights {
            // if user provides weights, then we send the predefined messages according to the weights endlessly
            Some(weights) => self.run_endless_producer(weights).await,
            // if user do not provide weights, then we just send the predefined messages one by one in order
            None => self.run_in_order_producer().await,
        }
    }

    async fn run_endless_producer(&self, weights: &[usize]) -> Result<(), overwatch_rs::DynError> {
        let dist = WeightedIndex::new(weights.iter())?;
        let mut rng = StdRng::seed_from_u64(self.config.seed);
        loop {
            let idx = dist.sample(&mut rng);
            tokio::time::sleep(self.config.duration).await;
            let msg = &self.config.predefined_messages[idx];
            match self
                .message_event
                .send(NetworkEvent::RawMessage(msg.clone()))
            {
                Ok(_) => {
                    tracing::debug!(
                        "sent message to \"{}\" to topic {}",
                        msg.payload,
                        msg.content_topic.content_topic_name
                    );
                }
                Err(e) => {
                    tracing::error!("error sending message: {:?}", e);
                }
            };
        }
    }

    async fn run_in_order_producer(&self) -> Result<(), overwatch_rs::DynError> {
        for msg in &self.config.predefined_messages {
            tokio::time::sleep(self.config.duration).await;
            match self
                .message_event
                .send(NetworkEvent::RawMessage(msg.clone()))
            {
                Ok(_) => {
                    tracing::debug!(
                        "sent message \"{}\" to topic {}",
                        msg.payload,
                        msg.content_topic.content_topic_name
                    );
                }
                Err(e) => {
                    tracing::error!("error sending message: {:?}", e);
                }
            };
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl NetworkBackend for Mock {
    type Settings = MockConfig;
    type State = NoState<MockConfig>;
    type Message = MockBackendMessage;
    type EventKind = EventKind;
    type NetworkEvent = NetworkEvent;

    fn new(config: Self::Settings, _: OverwatchHandle) -> Self {
        let message_event = broadcast::channel(BROADCAST_CHANNEL_BUF).0;

        Self {
            subscribed_topics: Arc::new(Mutex::new(HashSet::new())),
            messages: Arc::new(Mutex::new(
                config
                    .predefined_messages
                    .iter()
                    .map(|p| (p.content_topic.content_topic_name.to_string(), Vec::new()))
                    .collect(),
            )),
            message_event,
            config,
        }
    }

    async fn process(&self, msg: Self::Message) {
        match msg {
            MockBackendMessage::BootProducer { spawner } => {
                tracing::info!("booting producer");
                let this = self.clone();
                match (spawner)(Box::pin(async move { this.run_producer_handler().await })) {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!("error booting producer: {:?}", e);
                    }
                }
            }
            MockBackendMessage::Broadcast { topic, msg } => {
                tracing::info!("processed normal message");
                self.messages
                    .lock()
                    .unwrap()
                    .entry(topic)
                    .or_insert_with(Vec::new)
                    .push(msg.clone());
                let _ = self.message_event.send(NetworkEvent::RawMessage(msg));
            }
            MockBackendMessage::RelaySubscribe { topic } => {
                tracing::info!("processed relay subscription for topic: {topic}");
                self.subscribed_topics.lock().unwrap().insert(topic);
            }
            MockBackendMessage::RelayUnSubscribe { topic } => {
                tracing::info!("processed relay unsubscription for topic: {topic}");
                self.subscribed_topics.lock().unwrap().remove(&topic);
            }
            MockBackendMessage::Query { topic, tx } => {
                tracing::info!("processed query");
                let normal_msgs = self.messages.lock().unwrap();
                let msgs = normal_msgs.get(&topic).cloned().unwrap_or_default();
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

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use super::*;

    #[tokio::test]
    async fn test_mock_network() {
        let config = MockConfig {
            predefined_messages: vec![
                MockMessage {
                    payload: "foo".to_string(),
                    content_topic: MockContentTopic {
                        application_name: "mock network".into(),
                        version: 0,
                        content_topic_name: "foo".into(),
                    },
                    version: 0,
                    timestamp: 0,
                },
                MockMessage {
                    payload: "bar".to_string(),
                    content_topic: MockContentTopic {
                        application_name: "mock network".into(),
                        version: 0,
                        content_topic_name: "bar".into(),
                    },
                    version: 0,
                    timestamp: 0,
                },
            ],
            duration: tokio::time::Duration::from_secs(1),
            seed: 0,
            version: 1,
            weights: None,
        };

        let mock = Arc::new(Mock::new(
            config,
            OverwatchHandle::new(tokio::runtime::Handle::current(), mpsc::channel(1).0),
        ));
        // run producer
        let task = mock.clone();
        tokio::spawn(async move {
            task.run_producer_handler().await.unwrap();
        });

        static FOO_BROADCAST_MESSAGES: &[&str] = &["foo1", "foo2"];
        static BAR_BROADCAST_MESSAGES: &[&str] = &["bar1"];

        // broadcast
        for val in FOO_BROADCAST_MESSAGES {
            mock.process(MockBackendMessage::Broadcast {
                topic: "foo".to_string(),
                msg: MockMessage {
                    payload: val.to_string(),
                    content_topic: MockContentTopic {
                        application_name: "mock".into(),
                        version: 1,
                        content_topic_name: "foo content".into(),
                    },
                    version: 1,
                    timestamp: chrono::Utc::now().timestamp_nanos() as usize,
                },
            })
            .await;
        }

        for val in BAR_BROADCAST_MESSAGES {
            mock.process(MockBackendMessage::Broadcast {
                topic: "bar".to_string(),
                msg: MockMessage {
                    payload: val.to_string(),
                    content_topic: MockContentTopic {
                        application_name: "mock".into(),
                        version: 1,
                        content_topic_name: "bar content".into(),
                    },
                    version: 1,
                    timestamp: chrono::Utc::now().timestamp_nanos() as usize,
                },
            })
            .await;
        }

        // query
        let (qtx, qrx) = oneshot::channel();
        mock.process(MockBackendMessage::Query {
            topic: "foo".to_string(),
            tx: qtx,
        })
        .await;
        let query_result = qrx.await.unwrap();
        assert_eq!(query_result.len(), 2);
        for idx in 0..FOO_BROADCAST_MESSAGES.len() {
            assert_eq!(&query_result[idx].payload, FOO_BROADCAST_MESSAGES[idx]);
        }

        // subscribe
        mock.process(MockBackendMessage::RelaySubscribe {
            topic: "foo".to_string(),
        })
        .await;
        mock.process(MockBackendMessage::RelaySubscribe {
            topic: "bar".to_string(),
        })
        .await;
        assert!(mock.subscribed_topics.lock().unwrap().contains("foo"));
        assert!(mock.subscribed_topics.lock().unwrap().contains("bar"));

        // unsubscribe
        mock.process(MockBackendMessage::RelayUnSubscribe {
            topic: "foo".to_string(),
        })
        .await;
        assert!(!mock.subscribed_topics.lock().unwrap().contains("foo"));
        assert!(mock.subscribed_topics.lock().unwrap().contains("bar"));
        mock.process(MockBackendMessage::RelayUnSubscribe {
            topic: "bar".to_string(),
        })
        .await;
        assert!(!mock.subscribed_topics.lock().unwrap().contains("foo"));
        assert!(!mock.subscribed_topics.lock().unwrap().contains("bar"));
    }
}
