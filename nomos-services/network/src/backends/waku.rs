use super::*;
use overwatch_rs::services::state::NoState;
use serde::{Deserialize, Serialize};
use tokio::sync::{
    broadcast::{self, Receiver, Sender},
    oneshot,
};
use tracing::{debug, error};
use waku_bindings::*;

const BROADCAST_CHANNEL_BUF: usize = 16;

pub struct Waku {
    waku: WakuNodeHandle<Running>,
    message_event: Sender<NetworkEvent>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WakuInfo {
    pub listen_addresses: Option<Vec<Multiaddr>>,
    pub peer_id: Option<PeerId>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WakuConfig {
    #[serde(flatten)]
    inner: WakuNodeConfig,
    initial_peers: Vec<Multiaddr>,
}

/// Interaction with Waku node
#[derive(Debug)]
pub enum WakuBackendMessage {
    /// Send a message to the network
    Broadcast {
        message: WakuMessage,
        topic: Option<WakuPubSubTopic>,
    },
    /// Make a connection to peer at provided multiaddress
    ConnectPeer { addr: Multiaddr },
    /// Subscribe to a particular Waku topic
    RelaySubscribe { topic: WakuPubSubTopic },
    /// Unsubscribe from a particular Waku topic
    RelayUnsubscribe { topic: WakuPubSubTopic },
    /// Retrieve old messages from another peer
    StoreQuery {
        query: StoreQuery,
        peer_id: PeerId,
        response: oneshot::Sender<StoreResponse>,
    },
    /// Send a message using Waku Light Push
    LightpushPublish {
        message: WakuMessage,
        topic: Option<WakuPubSubTopic>,
        peer_id: PeerId,
    },
    Info {
        reply_channel: oneshot::Sender<WakuInfo>,
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
    type Settings = WakuConfig;
    type State = NoState<WakuConfig>;
    type Message = WakuBackendMessage;
    type EventKind = EventKind;
    type NetworkEvent = NetworkEvent;

    fn new(config: Self::Settings) -> Self {
        let waku = waku_new(Some(config.inner)).unwrap().start().unwrap();
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
                debug!("received message event");
                if tx
                    .send(NetworkEvent::RawMessage(msg_event.waku_message().clone()))
                    .is_err()
                {
                    debug!("no active receiver");
                }
            }
            _ => tracing::warn!("unsupported event"),
        });
        Self {
            waku,
            message_event,
        }
    }

    async fn process(&self, msg: Self::Message) {
        match msg {
            WakuBackendMessage::Broadcast { message, topic } => {
                match self.waku.relay_publish_message(&message, topic, None) {
                    Ok(id) => debug!(
                        "successfully broadcast message with id: {id}, raw contents: {:?}",
                        message.payload()
                    ),
                    Err(e) => tracing::error!(
                        "could not broadcast message due to {e}, raw contents {:?}",
                        message.payload()
                    ),
                }
            }
            WakuBackendMessage::ConnectPeer { addr } => {
                match self.waku.connect_peer_with_address(&addr, None) {
                    Ok(_) => debug!("successfully connected to {addr}"),
                    Err(e) => {
                        tracing::warn!("Could not connect to {addr}: {e}");
                    }
                }
            }
            WakuBackendMessage::LightpushPublish {
                message,
                topic,
                peer_id,
            } => match self.waku.lightpush_publish(&message, topic, peer_id, None) {
                Ok(id) => debug!(
                    "successfully published lighpush message with id: {id}, raw contents: {:?}",
                    message.payload()
                ),
                Err(e) => tracing::error!(
                    "could not publish lightpush message due to {e}, raw contents {:?}",
                    message.payload()
                ),
            },
            WakuBackendMessage::RelaySubscribe { topic } => {
                match self.waku.relay_subscribe(Some(topic.clone())) {
                    Ok(_) => debug!("successfully subscribed to topic {:?}", topic),
                    Err(e) => {
                        tracing::error!("could not subscribe to topic {:?} due to {e}", topic)
                    }
                }
            }
            WakuBackendMessage::RelayUnsubscribe { topic } => {
                match self.waku.relay_unsubscribe(Some(topic.clone())) {
                    Ok(_) => debug!("successfully unsubscribed to topic {:?}", topic),
                    Err(e) => {
                        tracing::error!("could not unsubscribe to topic {:?} due to {e}", topic)
                    }
                }
            }
            WakuBackendMessage::StoreQuery {
                query,
                peer_id,
                response,
            } => match self.waku.store_query(&query, &peer_id, None) {
                Ok(res) => {
                    debug!(
                        "successfully retrieved stored messages with options {:?}",
                        query
                    );
                    response
                        .send(res)
                        .unwrap_or_else(|_| error!("client hung up store query handle"));
                }
                Err(e) => {
                    error!(
                        "could not retrieve store messages due to {e}, options: {:?}",
                        query
                    )
                }
            },
            WakuBackendMessage::Info { reply_channel } => {
                let listen_addresses = self.waku.listen_addresses().ok();
                let peer_id = self.waku.peer_id().ok();
                if reply_channel
                    .send(WakuInfo {
                        listen_addresses,
                        peer_id,
                    })
                    .is_err()
                {
                    error!("could not send waku info");
                }
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
