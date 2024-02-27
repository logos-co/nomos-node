mod command;
mod config;
mod swarm;

// std
pub use self::command::{Command, Libp2pInfo};
use self::command::{SwarmCommand, Topic};
pub use self::config::Libp2pConfig;
use self::swarm::SwarmHandler;

// internal
use super::NetworkBackend;
use mixnet::client::{MessageQueue, MixClient};
use mixnet::node::{MixNode, PacketQueue};
use nomos_core::wire;
use nomos_libp2p::gossipsub;
pub use nomos_libp2p::libp2p::gossipsub::{Message, TopicHash};
use nomos_libp2p::libp2p::StreamProtocol;
use nomos_libp2p::libp2p_stream::IncomingStreams;
// crates
use overwatch_rs::{overwatch::handle::OverwatchHandle, services::state::NoState};
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc};

/// A Libp2p network backend broadcasts messages to the network with mixing packets through mixnet,
/// and receives messages broadcasted from the network.
pub struct Libp2p {
    message_queue: MessageQueue,
    events_tx: broadcast::Sender<Event>,
    swarm_commands_tx: mpsc::Sender<SwarmCommand>,
}

#[derive(Debug)]
pub enum EventKind {
    Message,
}

/// Events emitted from [`NomosLibp2p`], which users can subscribe
#[derive(Debug, Clone)]
pub enum Event {
    /// Received a message broadcasted
    Message(gossipsub::Message),
}

const BUFFER_SIZE: usize = 64;
const STREAM_PROTOCOL: StreamProtocol = StreamProtocol::new("/mixnet");

#[async_trait::async_trait]
impl NetworkBackend for Libp2p {
    type Settings = Libp2pConfig;
    type State = NoState<Libp2pConfig>;
    type Message = Command;
    type EventKind = EventKind;
    type NetworkEvent = Event;

    fn new(config: Self::Settings, overwatch_handle: OverwatchHandle) -> Self {
        let (swarm_commands_tx, swarm_commands_rx) = tokio::sync::mpsc::channel(BUFFER_SIZE);
        let (events_tx, _) = tokio::sync::broadcast::channel(BUFFER_SIZE);

        let mut swarm_handler = SwarmHandler::new(
            &config,
            swarm_commands_tx.clone(),
            swarm_commands_rx,
            events_tx.clone(),
        );

        // Run mixnode
        let (mixnode, packet_queue) = MixNode::new(config.mixnode_config).unwrap();
        let swarm_cmd_tx = swarm_commands_tx.clone();
        let queue = packet_queue.clone();
        overwatch_handle.runtime().spawn(async move {
            Self::run_mixnode(mixnode, queue, swarm_cmd_tx).await;
        });
        let incoming_streams = swarm_handler.incoming_streams(STREAM_PROTOCOL);
        let runtime_handle = overwatch_handle.runtime().clone();
        let queue = packet_queue.clone();
        overwatch_handle.runtime().spawn(async move {
            Self::handle_incoming_streams(incoming_streams, queue, runtime_handle).await;
        });

        // Run mixclient
        let (mixclient, message_queue) = MixClient::new(config.mixclient_config).unwrap();
        let swarm_cmd_tx = swarm_commands_tx.clone();
        overwatch_handle.runtime().spawn(async move {
            Self::run_mixclient(mixclient, packet_queue, swarm_cmd_tx).await;
        });

        // Run swarm to make progress
        overwatch_handle.runtime().spawn(async move {
            swarm_handler.run(config.initial_peers).await;
        });

        Self {
            message_queue,
            events_tx,
            swarm_commands_tx,
        }
    }

    async fn process(&self, msg: Self::Message) {
        match msg {
            Command::Broadcast { topic, message } => {
                let msg = MixnetMessage { topic, message };
                if let Err(e) = self.message_queue.send(msg.as_bytes()).await {
                    tracing::error!("failed to send messasge to mixclient: {e}");
                }
            }
            Command::Subscribe(topic) => {
                if let Err(e) = self
                    .swarm_commands_tx
                    .send(SwarmCommand::Subscribe(topic))
                    .await
                {
                    tracing::error!("failed to send command to swarm: {e:?}");
                }
            }
            Command::Unsubscribe(topic) => {
                if let Err(e) = self
                    .swarm_commands_tx
                    .send(SwarmCommand::Unsubscribe(topic))
                    .await
                {
                    tracing::error!("failed to send command to swarm: {e:?}");
                }
            }
            Command::Info { reply } => {
                if let Err(e) = self
                    .swarm_commands_tx
                    .send(SwarmCommand::Info { reply })
                    .await
                {
                    tracing::error!("failed to send command to swarm: {e:?}");
                }
            }
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

impl Libp2p {
    async fn run_mixnode(
        mut mixnode: MixNode,
        packet_queue: PacketQueue,
        swarm_commands_tx: mpsc::Sender<SwarmCommand>,
    ) {
        todo!()
    }

    async fn run_mixclient(
        mut mixclient: MixClient,
        packet_queue: PacketQueue,
        swarm_commands_tx: mpsc::Sender<SwarmCommand>,
    ) {
        todo!()
    }

    async fn handle_incoming_streams(
        mut incoming_streams: IncomingStreams,
        packet_queue: PacketQueue,
        runtime_handle: Handle,
    ) {
        todo!()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct MixnetMessage {
    topic: Topic,
    message: Box<[u8]>,
}

impl MixnetMessage {
    fn as_bytes(&self) -> Box<[u8]> {
        wire::serialize(self)
            .expect("Couldn't serialize MixnetMessage")
            .into_boxed_slice()
    }

    fn from_bytes(data: &[u8]) -> Result<Self, wire::Error> {
        wire::deserialize(data)
    }
}
