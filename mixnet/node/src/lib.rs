pub mod config;

use std::{error::Error, net::SocketAddr};

pub use config::MixnetNodeConfig;
use mixnet_protocol::Body;
use mixnet_topology::MixnetNodeId;
use nym_sphinx::{
    addressing::nodes::NymNodeRoutingAddress, Delay, DestinationAddressBytes, NodeAddressBytes,
    Payload, PrivateKey, PublicKey,
};
use sphinx_packet::{crypto::PUBLIC_KEY_SIZE, ProcessedPacket, SphinxPacket};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

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
        let body = Body::read_body(&mut socket).await?;
        match body {
            Body::SphinxPacket(packet) => Self::handle_sphinx_packet(private_key, packet).await,
            Body::FinalPayload(payload) => {
                Self::handle_final_payload(
                    private_key,
                    client_address,
                    Payload::from_bytes(&payload)?,
                )
                .await
            }
        }
    }

    async fn handle_sphinx_packet(
        private_key: PrivateKey,
        packet: Box<SphinxPacket>,
    ) -> Result<(), Box<dyn Error>> {
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
        _private_key: PrivateKey,
        client_address: SocketAddr,
        payload: Payload,
    ) -> Result<(), Box<dyn Error>> {
        // TODO: Reuse a conn instead of establishing ad-hoc conns
        let mut client_stream = TcpStream::connect(client_address).await?;
        // TODO: Decrypt the final payload using the private key
        client_stream.write_all(payload.as_bytes()).await?;
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
            Body::new_sphinx(packet),
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
            Body::new_final_borrowed(payload.as_bytes()),
            NymNodeRoutingAddress::try_from_bytes(&destination_addr.as_bytes())?,
        )
        .await
    }

    async fn forward(body: Body<'_>, to: NymNodeRoutingAddress) -> Result<(), Box<dyn Error>> {
        let addr = SocketAddr::try_from(to)?;

        let mut socket = TcpStream::connect(addr).await?;
        body.write_body(&mut socket).await?;

        Ok(())
    }
}
