mod client_notifier;
pub mod config;

use std::{error::Error, net::SocketAddr};

use client_notifier::ClientNotifier;
pub use config::MixnetNodeConfig;
use mixnet_protocol::Body;
use mixnet_topology::MixnetNodeId;
use nym_sphinx::{
    addressing::nodes::NymNodeRoutingAddress, Delay, DestinationAddressBytes, NodeAddressBytes,
    Payload, PrivateKey,
};
pub use sphinx_packet::crypto::PRIVATE_KEY_SIZE;
use sphinx_packet::{crypto::PUBLIC_KEY_SIZE, ProcessedPacket, SphinxPacket};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
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
        self.config.public_key()
    }

    const CLIENT_NOTI_CHANNEL_SIZE: usize = 100;

    pub async fn run(mut self) -> Result<(), Box<dyn Error>> {
        // Spawn a ClientNotifier
        let (client_tx, client_rx) = mpsc::channel(Self::CLIENT_NOTI_CHANNEL_SIZE);
        tokio::spawn(async move {
            if let Err(e) = ClientNotifier::run(self.config.client_listen_address, client_rx).await
            {
                tracing::error!("failed to run client notifier: {e}");
            }
        });

        //TODO: Accepting ad-hoc TCP conns for now. Improve conn handling.
        //TODO: Add graceful shutdown
        let listener = TcpListener::bind(self.config.listen_address).await?;
        // update the port if the port is assigned automatically by the system
        if self.config.listen_address.port() == 0 {
            self.config
                .listen_address
                .set_port(listener.local_addr().unwrap().port());
        }

        tracing::info!(
            "Listening mixnet node connections: {}",
            self.config.listen_address
        );

        loop {
            match listener.accept().await {
                Ok((socket, remote_addr)) => {
                    tracing::debug!("Accepted incoming connection from {remote_addr:?}");

                    let client_tx = client_tx.clone();
                    let private_key = PrivateKey::from(self.config.private_key);
                    tokio::spawn(async move {
                        if let Err(e) =
                            Self::handle_connection(socket, private_key, client_tx).await
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
        client_tx: mpsc::Sender<Body>,
    ) -> Result<(), Box<dyn Error>> {
        let body = Body::read(&mut socket).await?;
        match body {
            Body::SphinxPacket(packet) => Self::handle_sphinx_packet(private_key, packet).await,
            _body @ Body::FinalPayload(_) => {
                Self::forward_body_to_client_notifier(private_key, client_tx, _body).await
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

    async fn forward_body_to_client_notifier(
        _private_key: PrivateKey,
        client_tx: mpsc::Sender<Body>,
        body: Body,
    ) -> Result<(), Box<dyn Error>> {
        // TODO: Decrypt the final payload using the private key

        // Do not wait when the channel is full or no receiver exists
        client_tx.try_send(body)?;
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
            Body::new_final_payload(payload),
            NymNodeRoutingAddress::try_from_bytes(&destination_addr.as_bytes())?,
        )
        .await
    }

    async fn forward(body: Body, to: NymNodeRoutingAddress) -> Result<(), Box<dyn Error>> {
        let addr = SocketAddr::try_from(to)?;

        let mut socket = TcpStream::connect(addr).await?;
        body.write(&mut socket).await?;

        Ok(())
    }
}
