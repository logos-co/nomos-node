use super::*;
use ::waku::*;
use overwatch::services::state::NoState;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::{self, Receiver, Sender};

const BROADCAST_CHANNEL_BUF: usize = 16;

pub struct Waku {
    waku: WakuNodeHandle<Running>,
    message_event: Sender<NetworkEvent>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WakuConfig {
    #[serde(flatten)]
    inner: WakuNodeConfig,
    initial_peers: Vec<Multiaddr>,
}

#[derive(Debug)]
pub enum WakuBackendMessage {
    Broadcast {
        message: WakuMessage,
        topic: Option<WakuPubSubTopic>,
    },
}

#[derive(Debug)]
pub enum EventKind {
    Message,
}

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    RawMessage(WakuMessage),
}

#[async_trait::async_trait]
impl NetworkBackend for Waku {
    type Config = WakuConfig;
    type State = NoState<WakuConfig>;
    type Message = WakuBackendMessage;
    type EventKind = EventKind;
    type NetworkEvent = NetworkEvent;

    fn new(config: Self::Config) -> Self {
        let waku = waku_new(Some(config.inner)).unwrap().start().unwrap();
        waku.relay_subscribe(None).unwrap();
        tracing::info!("waku listening on {}", waku.listen_addresses().unwrap()[0]);
        for peer in &config.initial_peers {
            if let Err(e) = waku.connect_peer_with_address(peer, None) {
                tracing::warn!("Could not connect to {peer}: {e}");
            }
        }

        let message_event = broadcast::channel(BROADCAST_CHANNEL_BUF).0;
        let tx = message_event.clone();
        waku_set_event_callback(move |sig| match sig.event() {
            Event::WakuMessage(ref msg_event) => {
                tracing::debug!("received message event");
                if tx
                    .send(NetworkEvent::RawMessage(msg_event.waku_message().clone()))
                    .is_err()
                {
                    tracing::debug!("no active receiver");
                }
            }
            _ => tracing::warn!("unsupported event"),
        });
        Self {
            waku,
            message_event,
        }
    }

    async fn subscribe(&mut self, kind: Self::EventKind) -> Receiver<Self::NetworkEvent> {
        match kind {
            EventKind::Message => {
                tracing::debug!("processed subscription to incoming messages");
                self.message_event.subscribe()
            }
        }
    }

    async fn send(&self, msg: Self::Message) {
        match msg {
            WakuBackendMessage::Broadcast { message, topic } => {
                match self.waku.relay_publish_message(&message, topic, None) {
                    Ok(id) => tracing::debug!(
                        "successfully broadcast message with id: {id}, raw contents: {:?}",
                        message.payload()
                    ),
                    Err(e) => tracing::error!(
                        "could not broadcast message due to {e}, raw contents {:?}",
                        message.payload()
                    ),
                }
            }
        };
    }
}
