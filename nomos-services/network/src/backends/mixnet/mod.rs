use std::net::SocketAddr;

// internal
use super::{
    libp2p::{self, swarm::SwarmHandler, Libp2pConfig, Topic},
    NetworkBackend,
};
use futures::StreamExt;
use mixnet::{
    address::NodeAddress,
    client::{MessageQueue, MixClient, MixClientConfig},
    node::{MixNode, MixNodeConfig, Output, PacketQueue},
    packet::PacketBody,
};
use nomos_core::wire;
use nomos_libp2p::{
    libp2p::{Stream, StreamProtocol},
    libp2p_stream::IncomingStreams,
    Multiaddr, Protocol,
};
// crates
use overwatch_rs::{overwatch::handle::OverwatchHandle, services::state::NoState};
use serde::{Deserialize, Serialize};
use tokio::{
    runtime::Handle,
    sync::{broadcast, mpsc, oneshot},
};

/// A Mixnet network backend broadcasts messages to the network with mixing packets through mixnet,
/// and receives messages broadcasted from the network.
pub struct MixnetNetworkBackend {
    libp2p_events_tx: broadcast::Sender<libp2p::Event>,
    libp2p_commands_tx: mpsc::Sender<libp2p::Command>,

    mixclient_message_queue: MessageQueue,
}

#[derive(Clone, Debug)]
pub struct MixnetConfig {
    libp2p_config: Libp2pConfig,
    mixclient_config: MixClientConfig,
    mixnode_config: MixNodeConfig,
}

const BUFFER_SIZE: usize = 64;
const STREAM_PROTOCOL: StreamProtocol = StreamProtocol::new("/mixnet");

#[async_trait::async_trait]
impl NetworkBackend for MixnetNetworkBackend {
    type Settings = MixnetConfig;
    type State = NoState<MixnetConfig>;
    type Message = libp2p::Command;
    type EventKind = libp2p::EventKind;
    type NetworkEvent = libp2p::Event;

    fn new(config: Self::Settings, overwatch_handle: OverwatchHandle) -> Self {
        // TODO: One important task that should be spawned is
        // subscribing NewEntropy events that will be emitted from the consensus service soon.
        // so that new topology can be built internally.
        // In the mixnet spec, the robustness layer is responsible for this task.
        // We can implement the robustness layer in the mixnet-specific crate,
        // that we're going to define at the root of the project.

        let (libp2p_commands_tx, libp2p_commands_rx) = tokio::sync::mpsc::channel(BUFFER_SIZE);
        let (libp2p_events_tx, _) = tokio::sync::broadcast::channel(BUFFER_SIZE);

        let mut swarm_handler = SwarmHandler::new(
            &config.libp2p_config,
            libp2p_commands_tx.clone(),
            libp2p_commands_rx,
            libp2p_events_tx.clone(),
        );

        // Run mixnode
        let (mixnode, packet_queue) = MixNode::new(config.mixnode_config).unwrap();
        let libp2p_cmd_tx = libp2p_commands_tx.clone();
        let queue = packet_queue.clone();
        overwatch_handle.runtime().spawn(async move {
            Self::run_mixnode(mixnode, queue, libp2p_cmd_tx).await;
        });
        let incoming_streams = swarm_handler.incoming_streams(STREAM_PROTOCOL);
        let runtime_handle = overwatch_handle.runtime().clone();
        let queue = packet_queue.clone();
        overwatch_handle.runtime().spawn(async move {
            Self::handle_incoming_streams(incoming_streams, queue, runtime_handle).await;
        });

        // Run mixclient
        let (mixclient, message_queue) = MixClient::new(config.mixclient_config).unwrap();
        let libp2p_cmd_tx = libp2p_commands_tx.clone();
        overwatch_handle.runtime().spawn(async move {
            Self::run_mixclient(mixclient, packet_queue, libp2p_cmd_tx).await;
        });

        // Run libp2p swarm to make progress
        overwatch_handle.runtime().spawn(async move {
            swarm_handler.run(config.libp2p_config.initial_peers).await;
        });

        Self {
            libp2p_events_tx,
            libp2p_commands_tx,

            mixclient_message_queue: message_queue,
        }
    }

    async fn process(&self, msg: Self::Message) {
        match msg {
            libp2p::Command::Broadcast { topic, message } => {
                let msg = MixnetMessage { topic, message };
                if let Err(e) = self.mixclient_message_queue.send(msg.as_bytes()).await {
                    tracing::error!("failed to send messasge to mixclient: {e}");
                }
            }
            cmd => {
                if let Err(e) = self.libp2p_commands_tx.send(cmd).await {
                    tracing::error!("failed to send command to libp2p swarm: {e:?}");
                }
            }
        }
    }

    async fn subscribe(
        &mut self,
        kind: Self::EventKind,
    ) -> broadcast::Receiver<Self::NetworkEvent> {
        match kind {
            libp2p::EventKind::Message => self.libp2p_events_tx.subscribe(),
        }
    }
}

impl MixnetNetworkBackend {
    async fn run_mixnode(
        mut mixnode: MixNode,
        packet_queue: PacketQueue,
        swarm_commands_tx: mpsc::Sender<libp2p::Command>,
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
                                .send(libp2p::Command::Broadcast {
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
        swarm_commands_tx: mpsc::Sender<libp2p::Command>,
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
            match PacketBody::read_from(&mut stream).await? {
                Ok(packet_body) => {
                    packet_queue
                        .send(packet_body)
                        .await
                        .expect("The receiving half of packet queue should be always open");
                }
                Err(e) => {
                    tracing::error!(
                        "failed to parse packet body. continuing reading the next packet: {e}"
                    );
                }
            }
        }
    }

    async fn stream_send(
        addr: NodeAddress,
        packet_body: PacketBody,
        swarm_commands_tx: &mpsc::Sender<libp2p::Command>,
        packet_queue: &PacketQueue,
    ) {
        let (tx, rx) = oneshot::channel();
        swarm_commands_tx
            .send(libp2p::Command::Connect(libp2p::Dial {
                addr: Self::multiaddr_from(addr),
                retry_count: 3,
                result_sender: tx,
            }))
            .await
            .expect("Command receiver should be always open");

        match rx.await {
            Ok(Ok(peer_id)) => {
                swarm_commands_tx
                    .send(libp2p::Command::StreamSend {
                        peer_id,
                        protocol: STREAM_PROTOCOL,
                        packet_body,
                    })
                    .await
                    .expect("Command receiver should be always open");
            }
            Ok(Err(e)) => match e {
                nomos_libp2p::DialError::NoAddresses => {
                    tracing::debug!("Dialing failed because the peer is the local node. Sending msg directly to the queue");
                    packet_queue
                        .send(packet_body)
                        .await
                        .expect("The receiving half of packet queue should be always open");
                }
                _ => tracing::error!("failed to dial with unrecoverable error: {e}"),
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
pub struct MixnetMessage {
    pub topic: Topic,
    pub message: Box<[u8]>,
}

impl MixnetMessage {
    pub fn as_bytes(&self) -> Box<[u8]> {
        wire::serialize(self)
            .expect("Couldn't serialize MixnetMessage")
            .into_boxed_slice()
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, wire::Error> {
        wire::deserialize(data)
    }
}
