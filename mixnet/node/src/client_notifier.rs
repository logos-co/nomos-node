use std::net::SocketAddr;

use mixnet_protocol::{Body, ProtocolError};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

pub struct ClientNotifier {}

impl ClientNotifier {
    pub async fn run(
        listen_address: SocketAddr,
        mut rx: mpsc::Receiver<Body>,
    ) -> super::Result<()> {
        let listener = TcpListener::bind(listen_address)
            .await
            .map_err(super::ProtocolError::IO)?;
        tracing::info!("Listening mixnet client connections: {listen_address}");

        // Currently, handling only a single incoming connection
        // TODO: consider handling multiple clients
        loop {
            match listener.accept().await {
                Ok((socket, remote_addr)) => {
                    tracing::debug!("Accepted incoming client connection from {remote_addr}");

                    if let Err(e) = Self::handle_connection(socket, &mut rx).await {
                        tracing::error!("failed to handle conn: {e}");
                    }
                }
                Err(e) => tracing::warn!("Failed to accept incoming client connection: {e}"),
            }
        }
    }

    async fn handle_connection(
        mut socket: TcpStream,
        rx: &mut mpsc::Receiver<Body>,
    ) -> super::Result<()> {
        while let Some(body) = rx.recv().await {
            if let Err(e) = body.write(&mut socket).await {
                return Err(super::MixnetNodeError::Client(ProtocolError::from(e)));
            }
        }
        tracing::debug!("body receiver closed");
        Ok(())
    }
}
