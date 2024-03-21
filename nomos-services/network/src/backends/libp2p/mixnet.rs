use std::net::SocketAddr;

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
use serde::{Deserialize, Serialize};
use tokio::{
    runtime::Handle,
    sync::{mpsc, oneshot},
};

use crate::backends::libp2p::{Command, Dial, Topic};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MixnetConfig {
    pub mixclient: MixClientConfig,
    pub mixnode: MixNodeConfig,
}

pub(crate) const STREAM_PROTOCOL: StreamProtocol = StreamProtocol::new("/mixnet");

pub(crate) fn init_mixnet(
    config: MixnetConfig,
    runtime_handle: Handle,
    cmd_tx: mpsc::Sender<Command>,
    incoming_streams: IncomingStreams,
) -> MessageQueue {
    // Run mixnode
    let (mixnode, packet_queue) = MixNode::new(config.mixnode).unwrap();
    let libp2p_cmd_tx = cmd_tx.clone();
    let queue = packet_queue.clone();
    runtime_handle.spawn(async move {
        run_mixnode(mixnode, queue, libp2p_cmd_tx).await;
    });
    let handle = runtime_handle.clone();
    let queue = packet_queue.clone();
    runtime_handle.spawn(async move {
        handle_incoming_streams(incoming_streams, queue, handle).await;
    });

    // Run mixclient
    let (mixclient, message_queue) = MixClient::new(config.mixclient).unwrap();
    runtime_handle.spawn(async move {
        run_mixclient(mixclient, packet_queue, cmd_tx).await;
    });

    message_queue
}

async fn run_mixnode(
    mut mixnode: MixNode,
    packet_queue: PacketQueue,
    cmd_tx: mpsc::Sender<Command>,
) {
    while let Some(output) = mixnode.next().await {
        match output {
            Output::Forward(packet) => {
                stream_send(packet.address(), packet.body(), &cmd_tx, &packet_queue).await;
            }
            Output::ReconstructedMessage(message) => match MixnetMessage::from_bytes(&message) {
                Ok(msg) => {
                    cmd_tx
                        .send(Command::Broadcast {
                            topic: msg.topic,
                            message: msg.message,
                        })
                        .await
                        .unwrap();
                }
                Err(e) => {
                    tracing::error!("failed to parse message received from mixnet: {e}");
                }
            },
        }
    }
}

async fn run_mixclient(
    mut mixclient: MixClient,
    packet_queue: PacketQueue,
    cmd_tx: mpsc::Sender<Command>,
) {
    while let Some(packet) = mixclient.next().await {
        stream_send(packet.address(), packet.body(), &cmd_tx, &packet_queue).await;
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
            if let Err(e) = handle_stream(stream, queue).await {
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
    cmd_tx: &mpsc::Sender<Command>,
    packet_queue: &PacketQueue,
) {
    let (tx, rx) = oneshot::channel();
    cmd_tx
        .send(Command::Connect(Dial {
            addr: multiaddr_from(addr),
            retry_count: 3,
            result_sender: tx,
        }))
        .await
        .expect("Command receiver should be always open");

    match rx.await {
        Ok(Ok(peer_id)) => {
            cmd_tx
                .send(Command::StreamSend {
                    peer_id,
                    protocol: STREAM_PROTOCOL,
                    data: packet_body.bytes(),
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct MixnetMessage {
    pub topic: Topic,
    pub message: Box<[u8]>,
}

impl MixnetMessage {
    pub fn as_bytes(&self) -> Vec<u8> {
        wire::serialize(self).expect("Couldn't serialize MixnetMessage")
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, wire::Error> {
        wire::deserialize(data)
    }
}
