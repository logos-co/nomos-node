pub mod config;

use std::{error::Error, net::SocketAddr};

use config::{Config, MixNode, Topology};
use nym_sphinx_addressing::nodes::NymNodeRoutingAddress;
use rand::{rngs::OsRng, seq::IteratorRandom};
use sphinx_packet::{
    constants::{DESTINATION_ADDRESS_LENGTH, IDENTIFIER_LENGTH},
    crypto::{PrivateKey, PublicKey, PUBLIC_KEY_SIZE},
    header::delays::Delay,
    payload::{Payload, PAYLOAD_OVERHEAD_SIZE},
    route::{self, Destination, DestinationAddressBytes, NodeAddressBytes},
    ProcessedPacket, SphinxPacket, SphinxPacketBuilder,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{broadcast, mpsc},
};

pub struct Mixnet {
    config: Config,

    outbound_msg_rx: mpsc::Receiver<Message>,
    inbound_msg_tx: broadcast::Sender<Message>,

    topology: Topology,
    mixnode_rx: mpsc::Receiver<MixNode>,
}

pub type Message = Box<[u8]>;

impl Mixnet {
    pub fn new(
        config: Config,
        outbound_msg_rx: mpsc::Receiver<Message>,
        inbound_msg_tx: broadcast::Sender<Message>,
        mixnode_rx: mpsc::Receiver<MixNode>,
    ) -> Self {
        let topology = config.topology.clone();
        Self {
            config,
            outbound_msg_rx,
            inbound_msg_tx,
            topology,
            mixnode_rx,
        }
    }

    pub fn public_key(&self) -> [u8; PUBLIC_KEY_SIZE] {
        *PublicKey::from(&PrivateKey::from(self.config.private_key)).as_bytes()
    }

    pub fn external_address(&self) -> SocketAddr {
        self.config.external_address
    }

    pub async fn run(mut self) -> Result<(), Box<dyn Error>> {
        //TODO: using TCP for now. We may be able to make this package network-agnostic.
        let listener = TcpListener::bind(self.config.listen_address).await?;

        //TODO: add graceful shutdown
        loop {
            tokio::select! {
                connection = listener.accept() => {
                    match connection {
                        Ok((socket, remote_addr)) => {
                            tracing::debug!("Accepted incoming connection from {remote_addr:?}");

                            let private_key = PrivateKey::from(self.config.private_key);
                            let inbound_msg_tx = self.inbound_msg_tx.clone();
                            tokio::spawn(async {
                                if let Err(e) = Self::handle_connection(socket, private_key, inbound_msg_tx).await {
                                    tracing::error!("failed to handle conn: {e}");
                                }
                            });
                        }
                        Err(e) => tracing::warn!("Failed to accept incoming connection: {e}"),
                    }
                },
                Some(msg) = self.outbound_msg_rx.recv() => {
                    tracing::debug!("msg received from local: {msg:?}");

                    let topology = self.topology.clone();
                    let num_hops = self.config.num_hops as usize;
                    tokio::spawn(async move {
                        if let Err(e) = Self::send_to_mixnet(msg, topology, num_hops).await {
                            tracing::error!("failed to handle outbound msg: {e}");
                        }
                    });
                }
                Some(mixnode) = self.mixnode_rx.recv() => {
                    tracing::debug!("mixnode received");
                    self.topology.nodes.insert(mixnode.public_key, mixnode);
                }
            }
        }
    }

    async fn handle_connection(
        mut socket: TcpStream,
        private_key: PrivateKey,
        inbound_msg_tx: broadcast::Sender<Message>,
    ) -> Result<(), Box<dyn Error>> {
        tracing::debug!("handling connection");
        let packet_size = socket.read_u64().await?;
        let mut buf = vec![0; packet_size as usize]; // TODO: handle u64 -> usize failure
        socket.read_exact(&mut buf).await?;
        let packet = SphinxPacket::from_bytes(&buf)?;
        tracing::debug!("received a Sphinx packet from the TCP conn");

        match packet.process(&private_key)? {
            ProcessedPacket::ForwardHop(packet, next_node_addr, delay) => {
                Self::forward_to_next_hop(packet, next_node_addr, delay).await
            }
            ProcessedPacket::FinalHop(_, _, payload) => {
                Self::send_to_local(payload, inbound_msg_tx).await
            }
        }
    }

    async fn forward_to_next_hop(
        packet: Box<SphinxPacket>,
        next_node_addr: NodeAddressBytes,
        delay: Delay,
    ) -> Result<(), Box<dyn Error>> {
        tracing::debug!("Delaying the packet for {delay:?}");
        tokio::time::sleep(delay.to_duration()).await;
        Self::send_packet(packet, next_node_addr).await
    }

    async fn send_to_local(
        payload: Payload,
        inbound_msg_tx: broadcast::Sender<Message>,
    ) -> Result<(), Box<dyn Error>> {
        tracing::debug!("Sending the packet to the local");
        let message: Message = payload.recover_plaintext()?.into_boxed_slice();
        inbound_msg_tx.send(message)?;
        Ok(())
    }

    async fn send_to_mixnet(
        msg: Message,
        topology: Topology,
        num_hops: usize,
    ) -> Result<(), Box<dyn Error>> {
        tracing::debug!("Building a Sphinx packet: {num_hops}");
        let (packet, first_node) = Self::build_sphinx_packet(msg, topology, num_hops)?;
        Self::send_packet(Box::new(packet), first_node.address).await
    }

    fn build_sphinx_packet(
        msg: Message,
        topology: Topology,
        num_hops: usize,
    ) -> Result<(sphinx_packet::SphinxPacket, route::Node), Box<dyn Error>> {
        let route: Vec<route::Node> = topology
            .nodes
            .values()
            .choose_multiple(&mut OsRng, num_hops)
            .iter()
            .map(|&mixnode| mixnode.clone().try_into().unwrap())
            .collect();

        // TODO: a dummy destination for now
        // but we should pick a random destination that gathers all packets
        // and assembles them into a Message.
        let destination = Destination::new(
            DestinationAddressBytes::from_bytes([0; DESTINATION_ADDRESS_LENGTH]),
            [0; IDENTIFIER_LENGTH],
        );
        let delays: Vec<Delay> = route.iter().map(|_| Delay::new_from_nanos(0)).collect();

        //TODO: split a msg into fragments
        let packet = SphinxPacketBuilder::new()
            .with_payload_size(msg.len() + PAYLOAD_OVERHEAD_SIZE)
            .build_packet(msg, &route, &destination, &delays)?;

        let first_mixnode = route.first().cloned().expect("route is not empty");

        Ok((packet, first_mixnode))
    }

    async fn send_packet(
        packet: Box<SphinxPacket>,
        addr: NodeAddressBytes,
    ) -> Result<(), Box<dyn Error>> {
        let addr = SocketAddr::try_from(NymNodeRoutingAddress::try_from(addr)?)?;
        tracing::debug!("Sending a Sphinx packet to the node: {addr:?}");

        let mut socket = TcpStream::connect(addr).await?;

        socket.write_u64(packet.len() as u64).await?;
        socket.write_all(&packet.to_bytes()).await?;
        tracing::debug!("Sent a Sphinx packet successuflly to the node: {addr:?}");

        Ok(())
    }
}
