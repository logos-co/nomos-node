// internal
use super::{
    libp2p::{self, swarm::SwarmHandler, Libp2pConfig, Topic},
    NetworkBackend,
};
use mixnet::{
    client::{MessageQueue, MixClient, MixClientConfig},
    node::{MixNode, MixNodeConfig, PacketQueue},
};
use nomos_core::wire;
use nomos_libp2p::{libp2p::StreamProtocol, libp2p_stream::IncomingStreams};
// crates
use overwatch_rs::{overwatch::handle::OverwatchHandle, services::state::NoState};
use serde::{Deserialize, Serialize};
use tokio::{
    runtime::Handle,
    sync::{broadcast, mpsc},
};

/// A Mixnet network backend broadcasts messages to the network with mixing packets through mixnet,
/// and receives messages broadcasted from the network.
pub struct MixnetNetworkBackend {
    events_tx: broadcast::Sender<libp2p::Event>,
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
        let (libp2p_events_tx, libp2p_events_rx) = tokio::sync::broadcast::channel(BUFFER_SIZE);
        let (events_tx, _) = tokio::sync::broadcast::channel(BUFFER_SIZE);

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

        // Run event pipeline
        let evt_tx = events_tx.clone();
        overwatch_handle.runtime().spawn(async move {
            Self::pipe_events(libp2p_events_rx, evt_tx).await;
        });

        // Run libp2p swarm to make progress
        overwatch_handle.runtime().spawn(async move {
            swarm_handler.run(config.libp2p_config.initial_peers).await;
        });

        Self {
            events_tx,
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
            libp2p::EventKind::Message => self.events_tx.subscribe(),
        }
    }
}

impl MixnetNetworkBackend {
    async fn run_mixnode(
        mut mixnode: MixNode,
        packet_queue: PacketQueue,
        swarm_commands_tx: mpsc::Sender<libp2p::Command>,
    ) {
        todo!()
    }

    async fn run_mixclient(
        mut mixclient: MixClient,
        packet_queue: PacketQueue,
        swarm_commands_tx: mpsc::Sender<libp2p::Command>,
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

    /// Forwards events from libp2p swarm to the user of the [`MixnetNetworkBackend`].
    async fn pipe_events(
        mut libp2p_events_rx: broadcast::Receiver<libp2p::Event>,
        events_tx: broadcast::Sender<libp2p::Event>,
    ) {
        loop {
            match libp2p_events_rx.recv().await {
                Ok(event) => {
                    if let Err(e) = events_tx.send(event) {
                        tracing::error!("failed to send event to channel: {e}");
                    }
                }
                Err(e) => {
                    tracing::error!("failed to receive event from libp2p swarm: {e}");
                }
            }
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
