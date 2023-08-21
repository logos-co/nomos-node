use std::{error::Error, net::SocketAddr};

use mixnet_protocol::Body;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

pub struct ClientNotifier {}

impl ClientNotifier {
    pub async fn run(
        listen_address: SocketAddr,
        mut rx: mpsc::Receiver<Body>,
    ) -> Result<(), Box<dyn Error>> {
        let listener = TcpListener::bind(listen_address).await?;

        // Currently, handling only a single incoming connection
        // TODO: consider handling multiple clients
        loop {
            match listener.accept().await {
                Ok((socket, remote_addr)) => {
                    tracing::debug!("Accepted incoming client connection from {remote_addr:?}");

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
    ) -> Result<(), Box<dyn Error>> {
        while let Some(body) = rx.recv().await {
            if let Err(e) = body.write(&mut socket).await {
                return Err(format!("error from client conn: {e}").into());
            }
        }
        tracing::debug!("body receiver closed");
        Ok(())
    }
}
