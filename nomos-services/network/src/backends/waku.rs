// std
use std::fmt::Formatter;
use std::future::Future;
// crates
use futures::Stream;
use mixnet::{config::MixNode, Mixnet};
use serde::{Deserialize, Serialize};
use tokio::sync::{
    broadcast::{self, Receiver, Sender},
    mpsc, oneshot,
};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{debug, error};
// internal
use super::*;
use overwatch_rs::services::state::NoState;
use waku_bindings::*;

const BROADCAST_CHANNEL_BUF: usize = 16;

pub struct Waku {
    message_event: Sender<NetworkEvent>,
    backend_message_tx: mpsc::Sender<WakuBackendMessage>,
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
    pub mixnet: mixnet::config::Config, //TODO: make this optional
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

const BACKEND_MSG_CHANNEL_SIZE: usize = 100;

const MIXNET_CHANNEL_SIZE: usize = 100;
const MIXNET_TOPOLOGY_CHANNEL_SIZE: usize = 3;

// NOTE: MixnetMessage is to be serialized as JSON
// because 'wire' serialization fails with an error(SequenceMustHaveLength)
// due to #[serde(flatten)] in WakuMessage.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct MixnetMessage {
    message: WakuMessage,
    topic: Option<WakuPubSubTopic>,
}

#[async_trait::async_trait]
impl NetworkBackend for Waku {
    type Settings = WakuConfig;
    type State = NoState<WakuConfig>;
    type Message = WakuBackendMessage;
    type EventKind = EventKind;
    type NetworkEvent = NetworkEvent;

    fn new(mut config: Self::Settings, overwatch_handle: OverwatchHandle) -> Self {
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

        let (mixnet_outbound_msg_tx, mixnet_outbound_msg_rx) =
            mpsc::channel::<mixnet::Message>(MIXNET_CHANNEL_SIZE);
        let (mixnet_inbound_msg_tx, mut mixnet_inbound_msg_rx) =
            broadcast::channel::<mixnet::Message>(MIXNET_CHANNEL_SIZE);
        let (_mixnet_mixnode_tx, mixnet_mixnode_rx) =
            mpsc::channel::<MixNode>(MIXNET_TOPOLOGY_CHANNEL_SIZE);
        let mixnet = Mixnet::new(
            config.mixnet,
            mixnet_outbound_msg_rx,
            mixnet_inbound_msg_tx,
            mixnet_mixnode_rx,
        );

        overwatch_handle.runtime().spawn(async move {
            if let Err(e) = mixnet.run().await {
                tracing::error!("error from mixnet: {e}");
            }
        });

        let (backend_message_tx, mut backend_message_rx) = mpsc::channel(BACKEND_MSG_CHANNEL_SIZE);

        overwatch_handle.runtime().spawn(async move {
            loop {
                tokio::select! {
                    Some(msg) = backend_message_rx.recv() => {
                        handle_backend_message(&waku, msg, mixnet_outbound_msg_tx.clone()).await;
                    }
                    Ok(msg) = mixnet_inbound_msg_rx.recv() => {
                        let MixnetMessage { message, topic } = serde_json::from_slice(&msg).unwrap();
                        tracing::debug!("received MixnetMessage from mixnet: {message:?} {topic:?}");

                        match waku.relay_publish_message(&message, topic, None) {
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
                    //TODO: periodically broadcast mixnode information
                }
            }
        });

        Self {
            message_event,
            backend_message_tx,
        }
    }

    async fn process(&self, msg: Self::Message) {
        if let Err(e) = self.backend_message_tx.send(msg).await {
            tracing::error!("failed to send backend_message to waku: {e:?}");
        }
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

async fn handle_backend_message(
    waku: &WakuNodeHandle<Running>,
    msg: WakuBackendMessage,
    mixnet_outbound_msg_tx: mpsc::Sender<mixnet::Message>,
) {
    match msg {
        WakuBackendMessage::Broadcast { message, topic } => {
            tracing::debug!("sending MixnetMessage to mixnet");
            let msg = MixnetMessage { message, topic };
            let json_msg = serde_json::to_vec(&msg).unwrap();

            if let Err(e) = mixnet_outbound_msg_tx.send(json_msg.into()).await {
                tracing::error!("failed to send msg to mixnet: {e}");
            }
        }
        WakuBackendMessage::ConnectPeer { addr } => {
            match waku.connect_peer_with_address(&addr, None) {
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
        } => match waku.lightpush_publish(&message, topic, peer_id, None) {
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
            match waku.relay_subscribe(Some(topic.clone())) {
                Ok(_) => debug!("successfully subscribed to topic {:?}", topic),
                Err(e) => {
                    tracing::error!("could not subscribe to topic {:?} due to {e}", topic)
                }
            }
        }
        WakuBackendMessage::RelayUnsubscribe { topic } => {
            match waku.relay_unsubscribe(Some(topic.clone())) {
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
        } => match waku.store_query(&query, &peer_id, None) {
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
            let listen_addresses = waku.listen_addresses().ok();
            let peer_id = waku.peer_id().ok();
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
            let (stream, task) = waku_store_query_stream(waku, query);
            if reply_channel.send(Box::new(stream)).is_err() {
                error!("could not send archive subscribe stream");
            }
            task.await;
        }
    };
}

fn waku_store_query_stream(
    waku: &WakuNodeHandle<Running>,
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
        }) = waku.local_store_query(&query)
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
