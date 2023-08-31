// std
use std::{error::Error, ops::Range, time::Duration};
// internal
use super::NetworkBackend;
use mixnet_client::{MixnetClient, MixnetClientConfig};
use nomos_core::wire;
pub use nomos_libp2p::libp2p::gossipsub::{Message, TopicHash};
use nomos_libp2p::{
    libp2p::{gossipsub, Multiaddr, PeerId},
    BehaviourEvent, Swarm, SwarmConfig, SwarmEvent,
};
// crates
use overwatch_rs::{overwatch::handle::OverwatchHandle, services::state::NoState};
use rand::{rngs::OsRng, thread_rng, Rng};
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Libp2pConfig {
    #[serde(flatten)]
    pub inner: SwarmConfig,
    pub mixnet_client: MixnetClientConfig,
    pub mixnet_delay: Range<Duration>,
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

pub type Topic = String;
pub type CommandResultSender = oneshot::Sender<Result<(), Box<dyn Error + Send>>>;

/// Events emitted from [`NomosLibp2p`], which users can subscribe
#[derive(Debug, Clone)]
pub enum Event {
    Message(Message),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MixnetMessage {
    topic: Topic,
    message: Box<[u8]>,
}

impl MixnetMessage {
    pub fn as_bytes(&self) -> Vec<u8> {
        wire::serialize(self).expect("Couldn't serialize MixnetMessage")
    }
    pub fn from_bytes(data: &[u8]) -> Result<Self, wire::Error> {
        wire::deserialize(data)
    }
}

#[async_trait::async_trait]
impl NetworkBackend for Libp2p {
    type Settings = Libp2pConfig;
    type State = NoState<Libp2pConfig>;
    type Message = Command;
    type EventKind = EventKind;
    type NetworkEvent = Event;

    fn new(config: Self::Settings, overwatch_handle: OverwatchHandle) -> Self {
        let mut mixnet_client = MixnetClient::new(config.mixnet_client, OsRng);
        let (commands_tx, mut commands_rx) = tokio::sync::mpsc::channel(BUFFER_SIZE);
        let (events_tx, _) = tokio::sync::broadcast::channel(BUFFER_SIZE);
        let libp2p = Self {
            events_tx: events_tx.clone(),
            commands_tx,
        };
        overwatch_handle.runtime().spawn(async move {
            use tokio_stream::StreamExt;

            let Ok(mut stream) = mixnet_client.run().await else {
                tracing::error!("Could not quickstart mixnet stream");
                return;
            };

            let mut swarm = Swarm::build(&config.inner).unwrap();
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
                                tracing::debug!("sending message to mixnet client");
                                let msg = MixnetMessage { topic, message };
                                let delay = random_delay(&config.mixnet_delay);
                                log_error!(mixnet_client.send(msg.as_bytes(), delay));
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
                    Some(result) = stream.next() => {
                        match result {
                            Ok(msg) => {
                                tracing::debug!("receiving message from mixnet client");
                                let Ok(MixnetMessage { topic, message }) = MixnetMessage::from_bytes(&msg) else {
                                    tracing::error!("failed to deserialize msg received from mixnet client");
                                    continue;
                                };

                                match swarm.broadcast(&topic, message.to_vec()) {
                                    Ok(id) => {
                                        tracing::debug!("broadcasted message with id: {id} tp topic: {topic}");
                                    }
                                    Err(e) => {
                                        tracing::error!("failed to broadcast message to topic: {topic} {e:?}");
                                    }
                                }

                                // self-notification because libp2p doesn't do it
                                if swarm.is_subscribed(&topic) {
                                    log_error!(events_tx.send(Event::Message(Message {
                                        source: None,
                                        data: message.into(),
                                        sequence_number: None,
                                        topic: Swarm::topic_hash(&topic),
                                    })));
                                }
                            },
                            Err(e) => {
                                todo!("Handle mixclient error: {e}");
                            }
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

fn random_delay(range: &Range<Duration>) -> Duration {
    if range.start == range.end {
        return range.start;
    }
    thread_rng().gen_range(range.start, range.end)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::random_delay;

    #[test]
    fn test_random_delay() {
        assert_eq!(
            random_delay(&(Duration::ZERO..Duration::ZERO)),
            Duration::ZERO
        );

        let range = Duration::from_millis(10)..Duration::from_millis(100);
        let delay = random_delay(&range);
        assert!(range.start <= delay && delay < range.end);
    }
}
