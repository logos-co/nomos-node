use std::{collections::HashMap, sync::Arc};

use super::*;
use overwatch_rs::services::state::NoState;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::{self, Receiver, Sender};
use tracing::debug;

const BROADCAST_CHANNEL_BUF: usize = 16;

#[derive(Debug, Clone)]
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

pub struct Mock {
    #[allow(clippy::type_complexity)]
    weighted_messages: Arc<Mutex<HashMap<u64, Vec<(usize, String)>>>>,
    messages: Arc<Mutex<HashMap<u64, Vec<String>>>>,
    message_event: Sender<NetworkEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MockConfig {
    initial_peers: Vec<u64>,
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
    type Config = MockConfig;
    type State = NoState<MockConfig>;
    type Message = MockBackendMessage;
    type EventKind = EventKind;
    type NetworkEvent = NetworkEvent;

    fn new(config: Self::Config) -> Self {
        let message_event = broadcast::channel(BROADCAST_CHANNEL_BUF).0;
        Self {
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
