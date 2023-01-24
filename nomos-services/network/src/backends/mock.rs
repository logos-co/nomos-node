use std::{collections::HashMap, sync::Arc};

use super::*;
use overwatch_rs::services::state::NoState;
use parking_lot::Mutex;
use rand::distributions::DistString;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::{self, Receiver, Sender};
use tracing::debug;

const BROADCAST_CHANNEL_BUF: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MockMessage {
    Normal {
        topic: u64,
        msg: String,
    },
    Weighted {
        weight: usize,
        msg: String,
        topic: u64,
    },
}

impl MockMessage {
    pub fn topic(&self) -> u64 { 
        match self {
            Self::Normal { topic, .. } => *topic,
            Self::Weighted { topic, .. } => *topic,
        }
    }

    pub fn msg(&self) -> String {
        match self {
            Self::Normal { msg, .. } => msg.clone(),
            Self::Weighted { msg, .. } => msg.clone(),
        }
    }
}

pub struct Mock {
    #[allow(clippy::type_complexity)]
    weighted_messages: Arc<Mutex<HashMap<u64, Vec<(usize, String)>>>>,
    messages: Arc<Mutex<HashMap<u64, Vec<String>>>>,
    message_event: Sender<NetworkEvent>,
    subscribe_tx: tokio::sync::mpsc::UnboundedSender<u64>,
    unsubscribe_tx: tokio::sync::mpsc::UnboundedSender<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MockConfig {
    pub initial_peers: Vec<u64>,
    pub predefined_messages: Vec<MockMessage>,
    pub duration: std::time::Duration,
}

/// Interaction with Mock node
#[derive(Debug)]
pub enum MockBackendMessage {
    Normal {
        topic: u64,
        msg: String,
    },
    Weighted {
        topic: u64,
        weight: usize,
        msg: String,
    },
    RelaySubscribe {
        topic: u64,
    },
    RelayUnSubscribe {
        topic: u64,
    },
    Query {
        topic: u64,
        tx: oneshot::Sender<Vec<String>>,
    },
    QueryWeighted {
        topic: u64,
        tx: oneshot::Sender<Vec<(usize, String)>>,
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

#[async_trait::async_trait]
impl NetworkBackend for Mock {
    type Settings = MockConfig;
    type State = NoState<MockConfig>;
    type Message = MockBackendMessage;
    type EventKind = EventKind;
    type NetworkEvent = NetworkEvent;

    fn new(config: Self::Settings) -> Self {
        let message_event = broadcast::channel(BROADCAST_CHANNEL_BUF).0;

        // send predefined messages
        let tx = message_event.clone();
        MockMessageProducer::new(tx, config.predefined_messages, config.duration).run();
        let (subscribe_tx, subscribe_rx) = tokio::sync::mpsc::unbounded_channel();
        let (unsubscribe_tx, unsubscribe_rx) = tokio::sync::mpsc::unbounded_channel();

        // subscribe processor
        MockSubscriber::new(subscribe_rx, unsubscribe_rx).run();

        Self {
            subscribe_tx,
            unsubscribe_tx,
            messages: Arc::new(Mutex::new(
                config
                    .initial_peers
                    .iter()
                    .map(|p| (*p, Vec::new()))
                    .collect(),
            )),
            message_event,
            weighted_messages: Arc::new(Mutex::new(
                config
                    .initial_peers
                    .iter()
                    .map(|p| (*p, Vec::new()))
                    .collect(),
            )),
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
                    .send(NetworkEvent::RawMessage(MockMessage::Normal { topic, msg }));
            }
            MockBackendMessage::Weighted { topic, weight, msg } => {
                debug!("processed weighted message");
                let mut weighted_msgs = self.weighted_messages.lock();
                weighted_msgs
                    .entry(topic)
                    .or_insert_with(Vec::new)
                    .push((weight, msg.clone()));
                drop(weighted_msgs);
                let _ = self
                    .message_event
                    .send(NetworkEvent::RawMessage(MockMessage::Weighted {
                        weight,
                        msg,
                        topic,
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
                let normal_msgs = self.messages.lock();
                let msgs = normal_msgs.get(&topic).cloned().unwrap_or_default();
                drop(normal_msgs);
                let _ = tx.send(msgs);
            }
            MockBackendMessage::QueryWeighted { topic, tx } => {
                debug!("processed query");
                let weighted_msgs = self.weighted_messages.lock();
                let msgs = weighted_msgs.get(&topic).cloned().unwrap_or_default();
                drop(weighted_msgs);
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

struct MockMessageProducer {
    topic: u64,
    predefined_messages: Vec<MockMessage>,
    duration: std::time::Duration,
    tx: Sender<NetworkEvent>,
}

impl MockMessageProducer {
    fn new(
        tx: Sender<NetworkEvent>,
        predefined_messages: Vec<MockMessage>,
        duration: std::time::Duration,
    ) -> Self {
        Self {
            topic: predefined_messages.len() as u64,
            predefined_messages,
            duration,
            tx,
        }
    }

    fn run(self) {
        tokio::spawn(async move {
            for msg in self.predefined_messages {
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

            let mut topic = self.topic;
            loop {
                tokio::time::sleep(self.duration).await;
                let msg = if topic % 2 == 0 {
                    MockMessage::Normal {
                        topic,
                        msg: rand::distributions::Alphanumeric
                            .sample_string(&mut rand::thread_rng(), 16),
                    }
                } else {
                    MockMessage::Weighted {
                        topic,
                        weight: rand::random(),
                        msg: rand::distributions::Alphanumeric
                            .sample_string(&mut rand::thread_rng(), 16),
                    }
                };
                match self.tx.clone().send(NetworkEvent::RawMessage(msg)) {
                    Ok(peers) => {
                        tracing::debug!("sent message to {} peers", peers);
                    }
                    Err(e) => {
                        tracing::error!("error sending message: {:?}", e);
                    }
                };
                topic += 1;
            }
        });
    }
}

struct MockSubscriber {
    subscribed_topics: HashMap<u64, usize>,
    subscribe_rx: tokio::sync::mpsc::UnboundedReceiver<u64>,
    unsubscribe_rx: tokio::sync::mpsc::UnboundedReceiver<u64>,
}

impl MockSubscriber {
    fn new(
        subscribe_rx: tokio::sync::mpsc::UnboundedReceiver<u64>,
        unsubscribe_rx: tokio::sync::mpsc::UnboundedReceiver<u64>,
    ) -> Self {
        Self {
            subscribed_topics: Default::default(),
            subscribe_rx,
            unsubscribe_rx,
        }
    }

    fn run(mut self) {
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(topic) = self.subscribe_rx.recv() => {
                        tracing::debug!("subscribed to topic: {}", topic);
                        self.subscribed_topics.entry(topic).and_modify(|v| *v += 1).or_insert(1);

                    }
                    Some(topic) = self.unsubscribe_rx.recv() => {
                        tracing::debug!("unsubscribed from topic: {}", topic);
                        self.subscribed_topics.entry(topic).and_modify(|v| {
                            if *v == 0 {
                                panic!("unsubscribed from topic {} more times than subscribed", topic);
                            } else {
                                *v -= 1;
                            }
                        });
                    }
                }
            }
        });
    }
}
