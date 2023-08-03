// std
use std::error::Error;
// internal
use super::NetworkBackend;
use mixnet::{config::MixNode, Mixnet};
use nomos_libp2p::{
    libp2p::{
        gossipsub::{self, Message},
        Multiaddr, PeerId,
    },
    BehaviourEvent, Swarm, SwarmConfig, SwarmEvent,
};
// crates
use overwatch_rs::{overwatch::handle::OverwatchHandle, services::state::NoState};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, oneshot};

macro_rules! log_error {
    ($e:expr) => {
        if let Err(e) = $e {
            tracing::error!("error while processing {}: {e:?}", stringify!($e));
        }
    };
}

pub struct Libp2p {
    events_tx: broadcast::Sender<Event>,
    commands_tx: mpsc::Sender<Command>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Libp2pInfo {
    pub listen_addresses: Vec<Multiaddr>,
    pub n_peers: usize,
    pub n_connections: u32,
    pub n_pending_connections: u32,
}

#[derive(Debug)]
pub enum EventKind {
    Message,
}

const BUFFER_SIZE: usize = 64;

#[derive(Debug)]
pub enum Command {
    Connect(PeerId, Multiaddr),
    Broadcast { topic: Topic, message: Box<[u8]> },
    Subscribe(Topic),
    Unsubscribe(Topic),
    Info { reply: oneshot::Sender<Libp2pInfo> },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct MixnetMessage {
    topic: Topic,
    message: Vec<u8>,
}

impl MixnetMessage {
    pub fn as_bytes(&self) -> Box<[u8]> {
        wire::serialize(self).unwrap().into_boxed_slice()
    }
    pub fn from_bytes(data: &[u8]) -> Self {
        wire::deserialize(data).unwrap()
    }
}

pub type Topic = String;
pub type CommandResultSender = oneshot::Sender<Result<(), Box<dyn Error + Send>>>;

/// Events emitted from [`NomosLibp2p`], which users can subscribe
#[derive(Debug, Clone)]
pub enum Event {
    Message(Message),
}

const MIXNET_CHANNEL_SIZE: usize = 100;
const MIXNET_TOPOLOGY_CHANNEL_SIZE: usize = 3;
const MIXNET_TOPOLOGY_TOPIC: &str = "MixnetTopology";

#[async_trait::async_trait]
impl NetworkBackend for Libp2p {
    type Settings = SwarmConfig;
    type State = NoState<SwarmConfig>;
    type Message = Command;
    type EventKind = EventKind;
    type NetworkEvent = Event;

    fn new(config: Self::Settings, overwatch_handle: OverwatchHandle) -> Self {
        let (commands_tx, mut commands_rx) = tokio::sync::mpsc::channel(BUFFER_SIZE);
        let (events_tx, _) = tokio::sync::broadcast::channel(BUFFER_SIZE);
        let libp2p = Self {
            events_tx: events_tx.clone(),
            commands_tx,
        };

        let (mixnet_outbound_msg_tx, mixnet_outbound_msg_rx) =
            mpsc::channel::<mixnet::Message>(MIXNET_CHANNEL_SIZE);
        let (mixnet_inbound_msg_tx, mut mixnet_inbound_msg_rx) =
            broadcast::channel::<mixnet::Message>(MIXNET_CHANNEL_SIZE);
        let (mixnet_mixnode_tx, mixnet_mixnode_rx) =
            mpsc::channel::<MixNode>(MIXNET_TOPOLOGY_CHANNEL_SIZE);
        let mixnet = Mixnet::new(
            Default::default(), //TODO: use a proper config
            mixnet_outbound_msg_rx,
            mixnet_inbound_msg_tx,
            mixnet_mixnode_rx,
        );
        let mixnet_topology_topic_hash = gossipsub::IdentTopic::new(MIXNET_TOPOLOGY_TOPIC).hash();

        let mixnode = MixNode {
            public_key: mixnet.public_key(),
            addr: mixnet.external_address(),
        };

        overwatch_handle.runtime().spawn(async move {
            if let Err(e) = mixnet.run().await {
                tracing::error!("error from mixnet: {e}");
            }
        });

        overwatch_handle.runtime().spawn(async move {
            use tokio_stream::StreamExt;
            let mut swarm = Swarm::build(&config).unwrap();

            if let Err(e) = swarm.subscribe(MIXNET_TOPOLOGY_TOPIC) {
                tracing::error!("failed to start subscribing {MIXNET_TOPOLOGY_TOPIC}: {e}");
            }

            loop {
                tokio::select! {
                    Some(event) = swarm.next() => {
                        match event {
                            SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(gossipsub::Event::Message {
                                propagation_source: peer_id,
                                message_id: id,
                                message,
                            })) => {
                                tracing::debug!("Got message with id: {id} forwared from peer: {peer_id}");
                                if message.topic == mixnet_topology_topic_hash {
                                    let mixnode = MixNode::from_bytes(&message.data);
                                    log_error!(mixnet_mixnode_tx.send(mixnode).await);
                                } else {
                                    log_error!(events_tx.send(Event::Message(message)));
                                }
                            }
                            SwarmEvent::ConnectionEstablished {
                                peer_id,
                                connection_id,
                               ..
                            } => {
                                tracing::debug!("connected to peer: {peer_id} {connection_id:?}");
                            }
                            SwarmEvent::ConnectionClosed {
                                peer_id,
                                connection_id,
                                cause,
                                ..
                            } => {
                                tracing::debug!("connection closed from peer: {peer_id} {connection_id:?} due to {cause:?}");
                            }
                            SwarmEvent::OutgoingConnectionError {
                                peer_id,
                                connection_id,
                                error,
                                ..
                            } => {
                                tracing::debug!("failed to connect to peer: {peer_id:?} {connection_id:?} due to: {error}");
                            }
                            _ => {}
                        }
                    }
                    Some(command) = commands_rx.recv() => {
                        match command {
                            Command::Connect(peer_id, peer_addr) => {
                                tracing::debug!("connecting to peer: {peer_id} {peer_addr}");
                                log_error!(swarm.connect(peer_id, peer_addr));
                            }
                            Command::Broadcast { topic, message } => {
                                let msg = MixnetMessage { topic, message: message.to_vec() };
                                log_error!(mixnet_outbound_msg_tx.send(msg.as_bytes()).await);
                            }
                            Command::Subscribe(topic) => {
                                tracing::debug!("subscribing to topic: {topic}");
                                log_error!(swarm.subscribe(&topic));
                            }
                            Command::Unsubscribe(topic) => {
                                tracing::debug!("unsubscribing to topic: {topic}");
                                log_error!(swarm.unsubscribe(&topic));
                            }
                            Command::Info { reply } => {
                                let swarm = swarm.swarm();
                                let network_info = swarm.network_info();
                                let counters = network_info.connection_counters();
                                let info = Libp2pInfo {
                                    listen_addresses: swarm.listeners().cloned().collect(),
                                    n_peers: network_info.num_peers(),
                                    n_connections: counters.num_connections(),
                                    n_pending_connections: counters.num_pending(),
                                };
                                log_error!(reply.send(info));
                            }
                        };
                    }
                    Ok(msg) = mixnet_inbound_msg_rx.recv() => {
                        let MixnetMessage { topic, message } = MixnetMessage::from_bytes(&msg);
                        match swarm.broadcast(&topic, message.clone()) {
                            Ok(id) => {
                                tracing::debug!("broadcasted message with id: {id} tp topic: {topic}");
                            }
                            Err(e) => {
                                tracing::error!("failed to broadcast message to topic: {topic} {e:?}");
                            }
                        }

                        if swarm.is_subscribed(&topic) {
                            log_error!(events_tx.send(Event::Message(Message {
                                source: None,
                                data: message,
                                sequence_number: None,
                                topic: Swarm::topic_hash(&topic),
                            })));
                        }
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(60)) => {
                        tracing::debug!("broadcasting my mixnode information");

                        // TODO: we might need to broadcast this anonymously so that subscribers
                        // cannot aware the PeerId of the publisher.
                        // rust-libp2p provides this feature: https://docs.rs/libp2p-gossipsub/latest/libp2p_gossipsub/enum.MessageAuthenticity.html#variant.Anonymous.
                        if let Err(e) = swarm.broadcast(MIXNET_TOPOLOGY_TOPIC, mixnode.as_bytes().to_vec()) {
                            tracing::error!("failed to broadcast topology message: {e:?}");
                        }
                    }
                }
            }
        });

        libp2p
    }

    async fn process(&self, msg: Self::Message) {
        if let Err(e) = self.commands_tx.send(msg).await {
            tracing::error!("failed to send command to nomos-libp2p: {e:?}");
        }
    }

    async fn subscribe(
        &mut self,
        kind: Self::EventKind,
    ) -> broadcast::Receiver<Self::NetworkEvent> {
        match kind {
            EventKind::Message => {
                tracing::debug!("processed subscription to incoming messages");
                self.events_tx.subscribe()
            }
        }
    }
}
