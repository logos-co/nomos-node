pub mod config;

use std::{error::Error, net::SocketAddr};

pub use config::MixnetNodeConfig;
use mixnet_protocol::{write_body, BodyType};
use mixnet_topology::MixnetNodeId;
use nym_sphinx::{
    addressing::nodes::NymNodeRoutingAddress, Delay, DestinationAddressBytes, NodeAddressBytes,
    Payload, PrivateKey, PublicKey,
};
use sphinx_packet::{crypto::PUBLIC_KEY_SIZE, ProcessedPacket, SphinxPacket};
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
};

// A mix node that routes packets in the Mixnet.
pub struct MixnetNode {
    config: MixnetNodeConfig,
}

impl MixnetNode {
    pub fn new(config: MixnetNodeConfig) -> Self {
        Self { config }
    }

    pub fn id(&self) -> MixnetNodeId {
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
                    let client_address = self.config.client_address;
                    tokio::spawn(async move {
                        if let Err(e) =
                            Self::handle_connection(socket, private_key, client_address).await
                        {
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
        client_address: SocketAddr,
    ) -> Result<(), Box<dyn Error>> {
        match BodyType::from_u8(socket.read_u8().await?) {
            BodyType::SphinxPacket => Self::handle_sphinx_packet(socket, private_key).await,
            BodyType::FinalPayload => {
                Self::handle_final_payload(socket, private_key, client_address).await
            }
        }
    }

    async fn handle_sphinx_packet(
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

    async fn handle_final_payload(
        mut socket: TcpStream,
        _private_key: PrivateKey,
        client_address: SocketAddr,
    ) -> Result<(), Box<dyn Error>> {
        // TODO: Reuse a conn instead of establishing ad-hoc conns
        let mut client_stream = TcpStream::connect(client_address).await?;

        // TODO: Decrypt the final payload using the private key
        let _ = tokio::io::copy(&mut socket, &mut client_stream).await?;

        Ok(())
    }

    async fn forward_packet_to_next_hop(
        packet: Box<SphinxPacket>,
        next_node_addr: NodeAddressBytes,
        delay: Delay,
    ) -> Result<(), Box<dyn Error>> {
        tracing::debug!("Delaying the packet for {delay:?}");
        tokio::time::sleep(delay.to_duration()).await;

        Self::forward(
            BodyType::SphinxPacket,
            &packet.to_bytes(),
            NymNodeRoutingAddress::try_from(next_node_addr)?,
        )
        .await
    }

    async fn forward_payload_to_destination(
        payload: Payload,
        destination_addr: DestinationAddressBytes,
    ) -> Result<(), Box<dyn Error>> {
        tracing::debug!("Forwarding final payload to destination mixnode");

        Self::forward(
            BodyType::FinalPayload,
            payload.as_bytes(),
            NymNodeRoutingAddress::try_from_bytes(&destination_addr.as_bytes())?,
        )
        .await
    }

    async fn forward(
        body_type: BodyType,
        body: &[u8],
        to: NymNodeRoutingAddress,
    ) -> Result<(), Box<dyn Error>> {
        let addr = SocketAddr::try_from(to)?;

        let mut socket = TcpStream::connect(addr).await?;
        write_body(&mut socket, body_type, body).await?;

        Ok(())
    }
}
