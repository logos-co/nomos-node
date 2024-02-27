mod command;
mod config;
mod swarm;

use std::net::SocketAddr;

// std
pub use self::command::{Command, Libp2pInfo};
use self::command::{Dial, SwarmCommand, Topic};
pub use self::config::Libp2pConfig;
use self::swarm::SwarmHandler;

// internal
use super::NetworkBackend;
use futures::StreamExt;
use mixnet::address::NodeAddress;
use mixnet::client::{MessageQueue, MixClient};
use mixnet::node::{MixNode, Output, PacketQueue};
use nomos_core::wire;
pub use nomos_libp2p::libp2p::gossipsub::{Message, TopicHash};
use nomos_libp2p::libp2p::{Stream, StreamProtocol};
use nomos_libp2p::libp2p_stream::IncomingStreams;
use nomos_libp2p::{gossipsub, Multiaddr, Protocol};
// crates
use overwatch_rs::{overwatch::handle::OverwatchHandle, services::state::NoState};
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc, oneshot};

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
        while let Some(output) = mixnode.next().await {
            match output {
                Output::Forward(packet) => {
                    Self::stream_send(
                        packet.address(),
                        packet.body(),
                        &swarm_commands_tx,
                        &packet_queue,
                    )
                    .await;
                }
                Output::ReconstructedMessage(message) => {
                    match MixnetMessage::from_bytes(&message) {
                        Ok(msg) => {
                            swarm_commands_tx
                                .send(SwarmCommand::Broadcast {
                                    topic: msg.topic,
                                    message: msg.message,
                                })
                                .await
                                .unwrap();
                        }
                        Err(e) => {
                            tracing::error!("failed to parse message received from mixnet: {e}");
                        }
                    }
                }
            }
        }
    }

    async fn run_mixclient(
        mut mixclient: MixClient,
        packet_queue: PacketQueue,
        swarm_commands_tx: mpsc::Sender<SwarmCommand>,
    ) {
        while let Some(packet) = mixclient.next().await {
            Self::stream_send(
                packet.address(),
                packet.body(),
                &swarm_commands_tx,
                &packet_queue,
            )
            .await;
        }
    }

    async fn handle_incoming_streams(
        mut incoming_streams: IncomingStreams,
        packet_queue: PacketQueue,
        runtime_handle: Handle,
    ) {
        while let Some((_, stream)) = incoming_streams.next().await {
            let queue = packet_queue.clone();
            runtime_handle.spawn(async move {
                if let Err(e) = Self::handle_stream(stream, queue).await {
                    tracing::warn!("stream closed: {e}");
                }
            });
        }
    }

    async fn handle_stream(mut stream: Stream, packet_queue: PacketQueue) -> std::io::Result<()> {
        loop {
            let msg = SwarmHandler::stream_read(&mut stream).await?;
            packet_queue.send(msg).await.unwrap();
        }
    }

    async fn stream_send(
        addr: NodeAddress,
        data: Box<[u8]>,
        swarm_commands_tx: &mpsc::Sender<SwarmCommand>,
        packet_queue: &PacketQueue,
    ) {
        let addr = Self::multiaddr_from(addr);
        let (tx, rx) = oneshot::channel();
        swarm_commands_tx
            .send(SwarmCommand::Connect(Dial {
                addr: addr.clone(),
                retry_count: 3,
                result_sender: tx,
            }))
            .await
            .unwrap();

        match rx.await {
            Ok(result) => match result {
                Ok(peer_id) => {
                    swarm_commands_tx
                        .send(SwarmCommand::StreamSend {
                            peer_id,
                            protocol: STREAM_PROTOCOL,
                            message: data,
                        })
                        .await
                        .unwrap();
                }
                Err(e) => match e {
                    nomos_libp2p::DialError::NoAddresses => {
                        tracing::debug!("Dialing failed because the peer is the local node. Sending msg directly to the queue");
                        packet_queue.send(data).await.unwrap();
                    }
                    _ => tracing::error!("failed to dial with unrecoverable error: {e}"),
                },
            },
            Err(e) => {
                tracing::error!("channel closed before receiving: {e}");
            }
        }
    }

    fn multiaddr_from(addr: NodeAddress) -> Multiaddr {
        match SocketAddr::from(addr) {
            SocketAddr::V4(addr) => Multiaddr::empty()
                .with(Protocol::Ip4(*addr.ip()))
                .with(Protocol::Udp(addr.port()))
                .with(Protocol::QuicV1),
            SocketAddr::V6(addr) => Multiaddr::empty()
                .with(Protocol::Ip6(*addr.ip()))
                .with(Protocol::Udp(addr.port()))
                .with(Protocol::QuicV1),
        }
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
