use std::{error::Error, net::SocketAddr};

use config::Config;
use mixnet_topology::MixnodeId;
use nym_sphinx::{
    addressing::nodes::NymNodeRoutingAddress, Delay, DestinationAddressBytes, NodeAddressBytes,
    Payload, PrivateKey, PublicKey,
};
use sphinx_packet::{crypto::PUBLIC_KEY_SIZE, ProcessedPacket, SphinxPacket};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

pub mod config;

// A mix node that routes packets in the Mixnet.
pub struct Mixnode {
    config: Config,
}

impl Mixnode {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn id(&self) -> MixnodeId {
        self.public_key()
    }

    pub fn public_key(&self) -> [u8; PUBLIC_KEY_SIZE] {
        *PublicKey::from(&PrivateKey::from(self.config.private_key)).as_bytes()
    }

    pub async fn run(self) -> Result<(), Box<dyn Error>> {
        //TODO: Accepting ad-hoc TCP conns for now. Improve conn handling.
        //TODO: Add graceful shutdown
        let listener = TcpListener::bind(self.config.listen_address).await?;

        loop {
            match listener.accept().await {
                Ok((socket, remote_addr)) => {
                    tracing::debug!("Accepted incoming connection from {remote_addr:?}");

                    let private_key = PrivateKey::from(self.config.private_key);
                    tokio::spawn(async {
                        if let Err(e) = Self::handle_connection(socket, private_key).await {
                            tracing::error!("failed to handle conn: {e}");
                        }
                    });
                }
                Err(e) => tracing::warn!("Failed to accept incoming connection: {e}"),
            }
        }
    }

    async fn handle_connection(
        mut socket: TcpStream,
        private_key: PrivateKey,
    ) -> Result<(), Box<dyn Error>> {
        let mut buf = Vec::new();
        socket.read_to_end(&mut buf).await?;

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

    async fn forward_packet_to_next_hop(
        packet: Box<SphinxPacket>,
        next_node_addr: NodeAddressBytes,
        delay: Delay,
    ) -> Result<(), Box<dyn Error>> {
        tracing::debug!("Delaying the packet for {delay:?}");
        tokio::time::sleep(delay.to_duration()).await;

        let addr = SocketAddr::try_from(NymNodeRoutingAddress::try_from(next_node_addr)?)?;
        tracing::debug!("Forwarding a Sphinx packet to the mixnode: {addr:?}");

        let mut socket = TcpStream::connect(addr).await?;
        socket.write_all(&packet.to_bytes()).await?;

        Ok(())
    }

    async fn forward_payload_to_destination(
        payload: Payload,
        destination_addr: DestinationAddressBytes,
    ) -> Result<(), Box<dyn Error>> {
        tracing::debug!("Forwarding payload to destination");

        let addr = SocketAddr::try_from(NymNodeRoutingAddress::try_from_bytes(
            &destination_addr.as_bytes(),
        )?)?;
        tracing::debug!("Forwarding a Sphinx packet to the destination: {addr:?}");

        let mut socket = TcpStream::connect(addr).await?;
        socket.write_all(payload.as_bytes()).await?;

        Ok(())
    }
}
