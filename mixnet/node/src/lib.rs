mod client_notifier;
pub mod config;

use std::net::SocketAddr;

use client_notifier::ClientNotifier;
pub use config::MixnetNodeConfig;
use mixnet_protocol::{
    connection::{Command, ConnectionCommand, ConnectionPool},
    Body, ProtocolError,
};
use mixnet_topology::MixnetNodeId;
use nym_sphinx::{
    addressing::nodes::{NymNodeRoutingAddress, NymNodeRoutingAddressError},
    Delay, DestinationAddressBytes, NodeAddressBytes, PrivateKey,
};
pub use sphinx_packet::crypto::PRIVATE_KEY_SIZE;
use sphinx_packet::{crypto::PUBLIC_KEY_SIZE, ProcessedPacket, SphinxPacket};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot},
};

pub type Result<T> = core::result::Result<T, MixnetNodeError>;

#[derive(Debug, thiserror::Error)]
pub enum MixnetNodeError {
    #[error("{0}")]
    Protocol(#[from] ProtocolError),
    #[error("invalid routing address: {0}")]
    InvalidRoutingAddress(#[from] NymNodeRoutingAddressError),
    #[error("send error: fail to send {0} to client")]
    ClientSendError(#[from] tokio::sync::mpsc::error::TrySendError<Body>),
    #[error("client: {0}")]
    Client(ProtocolError),
}

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

    pub async fn run(self) -> Result<()> {
        tracing::info!("Public key: {:?}", self.public_key());

        // Spawn a ClientNotifier
        let (client_tx, client_rx) = mpsc::channel(Self::CLIENT_NOTI_CHANNEL_SIZE);
        tokio::spawn(async move {
            if let Err(e) = ClientNotifier::run(self.config.client_listen_address, client_rx).await
            {
                tracing::error!("failed to run client notifier: {e}");
            }
        });

        //TODO: Accepting ad-hoc TCP conns for now. Improve conn handling.
        let listener = TcpListener::bind(self.config.listen_address)
            .await
            .map_err(ProtocolError::IO)?;
        tracing::info!(
            "Listening mixnet node connections: {}",
            self.config.listen_address
        );

        // TODO: expose the conn pool config
        let runner = MixnetNodeRunner::new(self.config, client_tx);

        loop {
            tokio::select! {
                res = listener.accept() => {
                    match res {
                        Ok((socket, remote_addr)) => {
                            tracing::debug!("Accepted incoming connection from {remote_addr:?}");

                            let runner = runner.clone();
                            tokio::spawn(async move {
                                if let Err(e) = runner.handle_connection(socket).await {
                                    tracing::error!("failed to handle conn: {e}");
                                }
                            });
                        }
                        Err(e) => tracing::warn!("Failed to accept incoming connection: {e}"),
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("Shutting down...");
                    return Ok(());
                }
            }
        }
    }
}

#[derive(Clone)]
struct MixnetNodeRunner {
    config: MixnetNodeConfig,
    client_tx: mpsc::Sender<Body>,
    conn_pool: ConnectionPool,
}

impl MixnetNodeRunner {
    fn new(config: MixnetNodeConfig, client_tx: mpsc::Sender<Body>) -> Self {
        let conn_pool = ConnectionPool::new(config.connection_pool_config.clone());
        Self {
            config,
            client_tx,
            conn_pool,
        }
    }

    async fn handle_connection(&self, mut socket: TcpStream) -> Result<()> {
        loop {
            let body = Body::read(&mut socket).await?;
            let mut this = self.clone();
            tokio::spawn(async move {
                if let Err(e) = this.handle_body(body).await {
                    tracing::error!("failed to handle body: {e}");
                }
            });
        }
    }

    async fn handle_body(&mut self, body: Body) -> Result<()> {
        match body {
            Body::SphinxPacket(packet) => self.handle_sphinx_packet(packet).await,
            Body::FinalPayload(payload) => {
                self.forward_body_to_client_notifier(Body::FinalPayload(payload))
                    .await
            }
            _ => unreachable!(),
        }
    }

    async fn handle_sphinx_packet(&mut self, packet: Box<SphinxPacket>) -> Result<()> {
        match packet
            .process(&PrivateKey::from(self.config.private_key))
            .map_err(ProtocolError::InvalidSphinxPacket)?
        {
            ProcessedPacket::ForwardHop(packet, next_node_addr, delay) => {
                self.forward_packet_to_next_hop(Body::SphinxPacket(packet), next_node_addr, delay)
                    .await
            }
            ProcessedPacket::FinalHop(destination_addr, _, payload) => {
                self.forward_payload_to_destination(Body::FinalPayload(payload), destination_addr)
                    .await
            }
        }
    }

    async fn forward_body_to_client_notifier(&self, body: Body) -> Result<()> {
        // TODO: Decrypt the final payload using the private key, if it's encrypted

        // Do not wait when the channel is full or no receiver exists
        self.client_tx.try_send(body)?;
        Ok(())
    }

    async fn forward_packet_to_next_hop(
        &mut self,
        body: Body,
        next_node_addr: NodeAddressBytes,
        delay: Delay,
    ) -> Result<()> {
        tracing::debug!("Delaying the packet for {delay:?}");
        tokio::time::sleep(delay.to_duration()).await;

        self.forward(body, NymNodeRoutingAddress::try_from(next_node_addr)?)
            .await
    }

    async fn forward_payload_to_destination(
        &mut self,
        body: Body,
        destination_addr: DestinationAddressBytes,
    ) -> Result<()> {
        tracing::debug!("Forwarding final payload to destination mixnode");

        self.forward(
            body,
            NymNodeRoutingAddress::try_from_bytes(&destination_addr.as_bytes())?,
        )
        .await
    }

    async fn forward(&mut self, mut body: Body, to: NymNodeRoutingAddress) -> Result<()> {
        let mut num_tries = 0;
        loop {
            match self.forward_once(body, to).await {
                Ok(_) => return Ok(()),
                Err((body_, e)) => {
                    if num_tries < self.config.max_net_write_tries {
                        num_tries += 1;
                        tracing::warn!(
                            "failed to forward body. retrying({}/{})...: {}",
                            num_tries,
                            self.config.max_net_write_tries,
                            e
                        );
                        body = body_;
                    } else {
                        return Err(MixnetNodeError::from(ProtocolError::from(e)));
                    }
                }
            }
        }
    }
    async fn forward_once(
        &mut self,
        body: Body,
        to: NymNodeRoutingAddress,
    ) -> core::result::Result<(), (Body, std::io::Error)> {
        let (tx, rx) = oneshot::channel();
        let cmd = Command {
            addr: SocketAddr::from(to),
            command: ConnectionCommand::Write { body, tx },
        };
        self.conn_pool.submit(cmd);

        rx.await
            .expect("command result must be returned from connection pool")
    }
}
