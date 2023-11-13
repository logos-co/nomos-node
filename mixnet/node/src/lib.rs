mod client_notifier;
pub mod config;
mod forward_scheduler;
mod forwarder;
mod inbound_handler;

use client_notifier::ClientNotifier;
pub use config::MixnetNodeConfig;
use mixnet_protocol::{Body, ProtocolError};
use mixnet_topology::MixnetNodeId;
use nym_sphinx::addressing::nodes::NymNodeRoutingAddressError;
pub use sphinx_packet::crypto::PRIVATE_KEY_SIZE;
use sphinx_packet::crypto::PUBLIC_KEY_SIZE;
use tokio::{net::TcpListener, sync::mpsc};

use crate::{
    forward_scheduler::{ForwardScheduler, Packet},
    inbound_handler::InboundHandler,
};

pub type Result<T> = core::result::Result<T, MixnetNodeError>;

#[derive(Debug, thiserror::Error)]
pub enum MixnetNodeError {
    #[error("{0}")]
    Protocol(#[from] ProtocolError),
    #[error("invalid routing address: {0}")]
    InvalidRoutingAddress(#[from] NymNodeRoutingAddressError),
    #[error("send error: {0}")]
    PacketSendError(#[from] tokio::sync::mpsc::error::SendError<Packet>),
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

        let (tx, rx) = mpsc::unbounded_channel();

        let packet_forwarder = ForwardScheduler::new(rx, self.config);

        tokio::spawn(async move {
            packet_forwarder.run().await;
        });

        let inbound_handler = InboundHandler::new(self.config, client_tx, tx);

        loop {
            tokio::select! {
                res = listener.accept() => {
                    match res {
                        Ok((socket, remote_addr)) => {
                            tracing::debug!("Accepted incoming connection from {remote_addr:?}");

                            let inbound_handler = inbound_handler.clone();
                            tokio::spawn(async move {
                                if let Err(e) = inbound_handler.handle_connection(socket).await {
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
