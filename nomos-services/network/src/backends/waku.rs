// std
use std::fmt::Formatter;
use std::future::Future;
// crates
use futures::Stream;
use serde::{Deserialize, Serialize};
use tokio::sync::{
    broadcast::{self, Receiver, Sender},
    oneshot,
};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{debug, error};
// internal
use super::*;
use overwatch_rs::services::state::NoState;
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
    pub inner: WakuNodeConfig,
    pub initial_peers: Vec<Multiaddr>,
}

/// Interaction with Waku node
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
    /// Get a local cached stream of messages for a particular content topic
    ArchiveSubscribe {
        query: StoreQuery,
        reply_channel: oneshot::Sender<Box<dyn Stream<Item = WakuMessage> + Send + Sync + Unpin>>,
    },
    /// Retrieve old messages from another peer
    StoreQuery {
        query: StoreQuery,
        peer_id: PeerId,
        reply_channel: oneshot::Sender<StoreResponse>,
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

impl Debug for WakuBackendMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            WakuBackendMessage::Broadcast { message, .. } => f
                .debug_struct("WakuBackendMessage::Broadcast")
                .field("message", message)
                .finish(),
            WakuBackendMessage::ConnectPeer { addr } => f
                .debug_struct("WakuBackendMessage::ConnectPeer")
                .field("addr", addr)
                .finish(),
            WakuBackendMessage::RelaySubscribe { topic } => f
                .debug_struct("WakuBackendMessage::RelaySubscribe")
                .field("topic", topic)
                .finish(),
            WakuBackendMessage::RelayUnsubscribe { topic } => f
                .debug_struct("WakuBackendMessage::RelayUnsubscribe")
                .field("topic", topic)
                .finish(),
            WakuBackendMessage::ArchiveSubscribe { query, .. } => f
                .debug_struct("WakuBackendMessage::ArchiveSubscribe")
                .field("query", query)
                .finish(),
            WakuBackendMessage::StoreQuery { query, peer_id, .. } => f
                .debug_struct("WakuBackendMessage::StoreQuery")
                .field("query", query)
                .field("peer_id", peer_id)
                .finish(),
            WakuBackendMessage::LightpushPublish {
                message,
                topic,
                peer_id,
            } => f
                .debug_struct("WakuBackendMessage::LightpushPublish")
                .field("message", message)
                .field("topic", topic)
                .field("peer_id", peer_id)
                .finish(),
            WakuBackendMessage::Info { .. } => f.debug_struct("WakuBackendMessage::Info").finish(),
        }
    }
}

#[derive(Debug)]
pub enum EventKind {
    Message,
}

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    RawMessage(WakuMessage),
}

impl Waku {
    pub fn waku_store_query_stream(
        &self,
        mut query: StoreQuery,
    ) -> (
        impl Stream<Item = WakuMessage>,
        impl Future<Output = ()> + '_,
    ) {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let task = async move {
            while let Ok(StoreResponse {
                messages,
                paging_options,
            }) = self.waku.local_store_query(&query)
            {
                // send messages
                for message in messages {
                    // this could fail if the receiver is dropped
                    // break out of the loop in that case
                    if sender.send(message).is_err() {
                        break;
                    }
                }
                // stop queries if we do not have any more pages
                if let Some(paging_options) = paging_options {
                    query.paging_options = Some(paging_options);
                } else {
                    break;
                }
            }
        };
        (UnboundedReceiverStream::new(receiver), task)
    }
}

#[async_trait::async_trait]
impl NetworkBackend for Waku {
    type Settings = WakuConfig;
    type State = NoState<WakuConfig>;
    type Message = WakuBackendMessage;
    type EventKind = EventKind;
    type NetworkEvent = NetworkEvent;

    fn new(mut config: Self::Settings, _: OverwatchHandle) -> Self {
        // set store protocol to active at all times
        config.inner.store = Some(true);
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
                reply_channel,
            } => match self.waku.store_query(&query, &peer_id, None) {
                Ok(res) => {
                    debug!(
                        "successfully retrieved stored messages with options {:?}",
                        query
                    );
                    reply_channel
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
            WakuBackendMessage::ArchiveSubscribe {
                reply_channel,
                query,
            } => {
                let (stream, task) = self.waku_store_query_stream(query);
                if reply_channel.send(Box::new(stream)).is_err() {
                    error!("could not send archive subscribe stream");
                }
                task.await;
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
