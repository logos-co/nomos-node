use nomos_libp2p::{
    libp2p::{
        gossipsub::{self, Message},
        Multiaddr, PeerId,
    },
    BehaviourEvent, Swarm, SwarmConfig, SwarmEvent,
};
use overwatch_rs::{overwatch::handle::OverwatchHandle, services::state::NoState};
use tokio::sync::{broadcast, mpsc};

use super::NetworkBackend;

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
    pub n_pending: u32,
}

#[derive(Debug)]
pub enum EventKind {
    Message,
}
use std::error::Error;
use tokio::sync::oneshot;

const BUFFER_SIZE: usize = 16;

#[derive(Debug)]
pub enum Command {
    Connect(PeerId, Multiaddr),
    Broadcast { topic: Topic, message: Vec<u8> },
    Subscribe(Topic),
    Unsubscribe(Topic),
    Info { reply: oneshot::Sender<Libp2pInfo> },
}

pub type Topic = String;
pub type CommandResultSender = oneshot::Sender<Result<(), Box<dyn Error + Send>>>;

/// Events emitted from [`NomosLibp2p`], which users can subscribe
#[derive(Debug, Clone)]
pub enum Event {
    Message(Message),
}

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
        overwatch_handle.runtime().spawn(async move {
            use tokio_stream::StreamExt;
            let mut swarm = Swarm::build(&config).unwrap();
            loop {
                tokio::select! {
                    Some(event) = swarm.next() => {
                        match event {
                            SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(gossipsub::Event::Message {
                                propagation_source: peer_id,
                                message_id: id,
                                message,
                            })) => {
                                tracing::debug!("Got message with id: {id} from peer: {peer_id}");
                                log_error!(events_tx.send(Event::Message(message)));
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
                                match swarm.broadcast(&topic, message) {
                                    Ok(id) => {
                                        tracing::debug!("broadcasted message with id: {id} tp topic: {topic}");
                                    }
                                    Err(e) => {
                                        tracing::error!("failed to broadcast message to topic: {topic} {e:?}");
                                    }
                                }
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
                                    n_pending: counters.num_pending(),
                                };
                                log_error!(reply.send(info));
                            }
                        };
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
