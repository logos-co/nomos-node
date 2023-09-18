mod command;
mod config;

// std
pub use self::command::{Command, Libp2pInfo};
use self::command::{Dial, Topic};
pub use self::config::Libp2pConfig;
use std::{collections::HashMap, ops::Range, time::Duration};

// internal
use super::NetworkBackend;
use mixnet_client::MixnetClient;
use nomos_core::wire;
pub use nomos_libp2p::libp2p::gossipsub::{Message, TopicHash};
use nomos_libp2p::{gossipsub, libp2p::swarm::ConnectionId, BehaviourEvent, Swarm, SwarmEvent};
// crates
use overwatch_rs::{overwatch::handle::OverwatchHandle, services::state::NoState};
use rand::{rngs::OsRng, thread_rng, Rng};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};
use tokio_stream::StreamExt;

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

#[derive(Debug)]
pub enum EventKind {
    Message,
}

const BUFFER_SIZE: usize = 64;
// TODO: make this configurable
const BACKOFF: u64 = 5;
// TODO: make this configurable
const MAX_RETRY: usize = 3;

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
        let mixnet_client = MixnetClient::new(config.mixnet_client.clone(), OsRng);
        let (commands_tx, mut commands_rx) = tokio::sync::mpsc::channel(BUFFER_SIZE);
        let (events_tx, _) = tokio::sync::broadcast::channel(BUFFER_SIZE);

        let cmd_tx = commands_tx.clone();
        overwatch_handle.runtime().spawn(async move {
            let Ok(mut stream) = mixnet_client.run().await else {
                tracing::error!("Could not quickstart mixnet stream");
                return;
            };

            while let Some(result) = stream.next().await {
                match result {
                    Ok(msg) => {
                        tracing::debug!("receiving message from mixnet client");
                        let Ok(MixnetMessage { topic, message }) = MixnetMessage::from_bytes(&msg)
                        else {
                            tracing::error!(
                                "failed to deserialize json received from mixnet client"
                            );
                            continue;
                        };

                        cmd_tx
                            .send(Command::DirectBroadcastAndRetry {
                                topic,
                                message,
                                retry_count: 0,
                            })
                            .await
                            .unwrap_or_else(|_| tracing::error!("could not schedule broadcast"));
                    }
                    Err(e) => {
                        todo!("Handle mixclient error: {e}");
                    }
                }
            }
        });
        let cmd_tx = commands_tx.clone();
        let notify = events_tx.clone();
        overwatch_handle.runtime().spawn(async move {
            let mut swarm = Swarm::build(&config.inner).unwrap();
            let mut mixnet_client = MixnetClient::new(config.mixnet_client, OsRng);

            // Keep the dialing history since swarm.connect doesn't return the result synchronously
            // TODO: Refactor: have a proper struct which holds this
            let mut pending_dials = HashMap::<ConnectionId, Dial>::new();

            for initial_peer in config.initial_peers {
                let dial = Dial { addr: initial_peer, retry_count: 0 };
                schedule_connect(dial, cmd_tx.clone()).await;
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
                                tracing::debug!("Got message with id: {id} from peer: {peer_id}");
                                log_error!(notify.send(Event::Message(message)));
                            }
                            SwarmEvent::ConnectionEstablished {
                                peer_id,
                                connection_id,
                                endpoint,
                               ..
                            } => {
                                tracing::debug!("connected to peer:{peer_id}, connection_id:{connection_id:?}");
                                if endpoint.is_dialer() {
                                    complete_connect(connection_id, &mut pending_dials);
                                }
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
                                tracing::error!("Failed to connect to peer: {peer_id:?} {connection_id:?} due to: {error}");
                                retry_connect(connection_id, &mut pending_dials, cmd_tx.clone());
                            }
                            _ => {}
                        }
                    }
                    Some(command) = commands_rx.recv() => {
                        match command {
                            Command::Connect(dial) => {
                                connect(&mut swarm, dial, &mut pending_dials);
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
                            Command::DirectBroadcastAndRetry { topic, message, retry_count } => {
                               broadcast_and_retry(topic, message, retry_count, cmd_tx.clone(), &mut swarm, notify.clone()).await;
                            }
                        };
                    }
                }
            }
        });
        Self {
            events_tx,
            commands_tx,
        }
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

async fn schedule_connect(dial: Dial, commands_tx: mpsc::Sender<Command>) {
    commands_tx
        .send(Command::Connect(dial))
        .await
        .unwrap_or_else(|_| tracing::error!("could not schedule connect"));
}

fn connect(swarm: &mut Swarm, dial: Dial, pending_dials: &mut HashMap<ConnectionId, Dial>) {
    tracing::debug!("Connecting to {}", dial.addr);

    match swarm.connect(dial.addr.clone()) {
        Ok(connection_id) => {
            // Dialing has been scheduled. The result will be notified as a SwarmEvent.
            pending_dials.insert(connection_id, dial);
        }
        Err(e) => {
            tracing::error!(
                "Failed to connect to {} with unretriable error: {e}",
                dial.addr
            );
        }
    }
}

fn complete_connect(connection_id: ConnectionId, pending_dials: &mut HashMap<ConnectionId, Dial>) {
    pending_dials.remove(&connection_id);
}

fn retry_connect(
    connection_id: ConnectionId,
    pending_dials: &mut HashMap<ConnectionId, Dial>,
    commands_tx: mpsc::Sender<Command>,
) {
    if let Some(mut dial) = pending_dials.remove(&connection_id) {
        dial.retry_count += 1;
        if dial.retry_count > MAX_RETRY {
            tracing::debug!("Max retry({MAX_RETRY}) has been reached: {dial:?}");
            return;
        }

        let wait = exp_backoff(dial.retry_count);
        tracing::debug!("Retry dialing in {wait:?}: {dial:?}");

        tokio::spawn(async move {
            tokio::time::sleep(wait).await;
            schedule_connect(dial, commands_tx).await;
        });
    }
}

async fn broadcast_and_retry(
    topic: Topic,
    message: Box<[u8]>,
    retry_count: usize,
    commands_tx: mpsc::Sender<Command>,
    swarm: &mut Swarm,
    events_tx: broadcast::Sender<Event>,
) {
    tracing::debug!("broadcasting message to topic: {topic}");

    match swarm.broadcast(&topic, message.to_vec()) {
        Ok(id) => {
            tracing::debug!("broadcasted message with id: {id} tp topic: {topic}");
            // self-notification because libp2p doesn't do it
            if swarm.is_subscribed(&topic) {
                log_error!(events_tx.send(Event::Message(Message {
                    source: None,
                    data: message.into(),
                    sequence_number: None,
                    topic: Swarm::topic_hash(&topic),
                })));
            }
        }
        Err(gossipsub::PublishError::InsufficientPeers) if retry_count < MAX_RETRY => {
            let wait = exp_backoff(retry_count);
            tracing::error!("failed to broadcast message to topic due to insufficient peers, trying again in {wait:?}");

            tokio::spawn(async move {
                tokio::time::sleep(wait).await;
                commands_tx
                    .send(Command::DirectBroadcastAndRetry {
                        topic,
                        message,
                        retry_count: retry_count + 1,
                    })
                    .await
                    .unwrap_or_else(|_| tracing::error!("could not schedule retry"));
            });
        }
        Err(e) => {
            tracing::error!("failed to broadcast message to topic: {topic} {e:?}");
        }
    }
}

fn exp_backoff(retry: usize) -> Duration {
    std::time::Duration::from_secs(BACKOFF.pow(retry as u32))
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
