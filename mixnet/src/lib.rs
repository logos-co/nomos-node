pub mod config;

use std::{
    error::Error,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use config::{Config, MixNode, Topology};
use nym_sphinx::{
    addressing::nodes::NymNodeRoutingAddress,
    chunking::{fragment::Fragment, reconstruction::MessageReconstructor},
    message::{NymMessage, PaddedMessage},
    params::PacketSize,
};
use rand::{rngs::OsRng, seq::IteratorRandom};
use sphinx_packet::{
    constants::IDENTIFIER_LENGTH,
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

    message_reconstructor: Arc<Mutex<MessageReconstructor>>,
}

pub type Message = Box<[u8]>;

enum TcpStreamBodyType {
    SphinxPacket,
    FinalPayload,
}

impl TcpStreamBodyType {
    fn as_u8(&self) -> u8 {
        match self {
            Self::SphinxPacket => 0,
            Self::FinalPayload => 1,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::SphinxPacket,
            1 => Self::FinalPayload,
            _ => todo!("return error"),
        }
    }
}

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
            message_reconstructor: Default::default(),
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
                            let message_reconstructor = self.message_reconstructor.clone();
                            tokio::spawn(async {
                                if let Err(e) = Self::handle_connection(socket, private_key, inbound_msg_tx, message_reconstructor).await {
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
        message_reconstructor: Arc<Mutex<MessageReconstructor>>,
    ) -> Result<(), Box<dyn Error>> {
        tracing::debug!("handling connection");

        match TcpStreamBodyType::from_u8(socket.read_u8().await?) {
            TcpStreamBodyType::SphinxPacket => {
                Self::handle_sphinx_packet_connection(socket, private_key).await
            }
            TcpStreamBodyType::FinalPayload => {
                Self::handle_final_payload_connection(
                    socket,
                    private_key,
                    inbound_msg_tx,
                    message_reconstructor,
                )
                .await
            }
        }
    }

    async fn handle_sphinx_packet_connection(
        mut socket: TcpStream,
        private_key: PrivateKey,
    ) -> Result<(), Box<dyn Error>> {
        tracing::debug!("handling sphinx packet connection");

        let packet_size = socket.read_u64().await?;
        let mut buf = vec![0; packet_size as usize]; // TODO: handle u64 -> usize failure
        socket.read_exact(&mut buf).await?;

        let packet = SphinxPacket::from_bytes(&buf)?;
        tracing::debug!("received a Sphinx packet from the TCP conn");

        match packet.process(&private_key)? {
            ProcessedPacket::ForwardHop(packet, next_node_addr, delay) => {
                Self::forward_packet_to_next_hop(packet, next_node_addr, delay).await
            }
            ProcessedPacket::FinalHop(destination_addr, _, payload) => {
                Self::forward_payload_to_destination(payload, destination_addr).await
            }
        }
    }

    async fn handle_final_payload_connection(
        mut socket: TcpStream,
        _private_key: PrivateKey,
        inbound_msg_tx: broadcast::Sender<Message>,
        message_reconstructor: Arc<Mutex<MessageReconstructor>>,
    ) -> Result<(), Box<dyn Error>> {
        tracing::debug!("handling final payload connection");

        let payload_size = socket.read_u64().await?;
        let mut buf = vec![0; payload_size as usize]; // TODO: handle u64 -> usize failure
        socket.read_exact(&mut buf).await?;

        let payload = Payload::from_bytes(&buf)?.recover_plaintext()?;
        let fragment = Fragment::try_from_bytes(&payload)?;

        if let Some((padded_message, _)) = {
            let mut reconstructor = message_reconstructor.lock().unwrap();
            reconstructor.insert_new_fragment(fragment)
        } {
            tracing::debug!("sending a reconstructed message to the local");
            let message = Self::remove_padding(padded_message)?;
            inbound_msg_tx.send(message)?;
        }

        Ok(())
    }

    async fn forward_packet_to_next_hop(
        packet: Box<SphinxPacket>,
        next_node_addr: NodeAddressBytes,
        delay: Delay,
    ) -> Result<(), Box<dyn Error>> {
        tracing::debug!("Delaying the packet for {delay:?}");
        tokio::time::sleep(delay.to_duration()).await;

        Self::send_packet(packet, next_node_addr).await
    }

    async fn forward_payload_to_destination(
        payload: Payload,
        destination_addr: DestinationAddressBytes,
    ) -> Result<(), Box<dyn Error>> {
        tracing::debug!("Forwarding payload to destination");

        Self::send_payload(
            payload,
            NodeAddressBytes::from_bytes(destination_addr.as_bytes()),
        )
        .await
    }

    async fn send_to_mixnet(
        msg: Message,
        topology: Topology,
        num_hops: usize,
    ) -> Result<(), Box<dyn Error>> {
        let dest_node: route::Node = topology
            .nodes
            .values()
            .choose(&mut OsRng)
            .expect("topology is not empty")
            .clone()
            .try_into()
            .unwrap();
        let destination = Destination::new(
            DestinationAddressBytes::from_bytes(dest_node.address.as_bytes()),
            [0; IDENTIFIER_LENGTH], // TODO: use a proper SURBIdentifier if we need SURB
        );

        let mut packets: Vec<(sphinx_packet::SphinxPacket, route::Node)> = Vec::new();

        for fragment in Self::pad_and_split_message(msg) {
            packets.push(Self::build_sphinx_packet(
                fragment,
                &destination,
                topology.clone(),
                num_hops,
            )?);
        }

        for (packet, first_node) in packets {
            tokio::spawn(async move {
                if let Err(e) = Self::send_packet(Box::new(packet), first_node.address).await {
                    tracing::error!("failed to send packet to the first node: {e}");
                }
            });
        }

        Ok(())
    }

    fn pad_and_split_message(msg: Message) -> Vec<Fragment> {
        let nym_message = NymMessage::new_plain(msg.into_vec());

        // TODO: add PUBLIC_KEY_SIZE for encryption for the destination
        // TODO: add ACK_OVERHEAD if we need SURB-ACKs.
        // https://github.com/nymtech/nym/blob/3748ab77a132143d5fd1cd75dd06334d33294815/common/nymsphinx/src/message.rs#L181-L181
        let plaintext_size_per_packet = PacketSize::RegularPacket.plaintext_size();

        nym_message
            .pad_to_full_packet_lengths(plaintext_size_per_packet)
            .split_into_fragments(&mut OsRng, plaintext_size_per_packet)
    }

    fn remove_padding(msg: Vec<u8>) -> Result<Message, Box<dyn Error>> {
        let padded_message = PaddedMessage::new_reconstructed(msg);
        // we need this because PaddedMessage.remove_padding requires it for other NymMessage types.
        let dummy_num_mix_hops = 0;

        match padded_message.remove_padding(dummy_num_mix_hops)? {
            NymMessage::Plain(msg) => Ok(msg.into_boxed_slice()),
            _ => todo!("return error"),
        }
    }

    fn build_sphinx_packet(
        fragment: Fragment,
        destination: &Destination,
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

        let delays: Vec<Delay> = route.iter().map(|_| Delay::new_from_nanos(0)).collect();

        // TODO: encryption for the destination
        // https://github.com/nymtech/nym/blob/3748ab77a132143d5fd1cd75dd06334d33294815/common/nymsphinx/src/preparer/payload.rs#L70
        let payload = fragment.into_bytes();

        let packet = SphinxPacketBuilder::new()
            .with_payload_size(payload.len() + PAYLOAD_OVERHEAD_SIZE)
            .build_packet(payload, &route, destination, &delays)?;

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

        socket
            .write_u8(TcpStreamBodyType::SphinxPacket.as_u8())
            .await?;
        socket.write_u64(packet.len() as u64).await?;
        socket.write_all(&packet.to_bytes()).await?;
        tracing::debug!("Sent a Sphinx packet successuflly to the node: {addr:?}");

        Ok(())
    }

    async fn send_payload(payload: Payload, addr: NodeAddressBytes) -> Result<(), Box<dyn Error>> {
        let addr = SocketAddr::try_from(NymNodeRoutingAddress::try_from(addr)?)?;
        tracing::debug!("Sending a payload to the node: {addr:?}");

        let mut socket = TcpStream::connect(addr).await?;

        socket
            .write_u8(TcpStreamBodyType::FinalPayload.as_u8())
            .await?;
        socket.write_u64(payload.len() as u64).await?;
        socket.write_all(payload.as_bytes()).await?;
        tracing::debug!("Sent a Sphinx packet successuflly to the node: {addr:?}");

        Ok(())
    }
}
