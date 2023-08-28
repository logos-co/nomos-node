use mixnet_protocol::Body;
use std::error::Error;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

pub struct ClientNotifier {}

impl ClientNotifier {
    pub async fn run(
        ln: TcpListener,
        mut rx: mpsc::Receiver<Body>,
        shutdown_rx: async_channel::Receiver<()>,
    ) -> Result<(), Box<dyn Error>> {
        // Currently, handling only a single incoming connection
        // TODO: consider handling multiple clients
        loop {
            tokio::select! {
                socket = ln.accept() => {
                    match socket {
                        Ok((socket, remote_addr)) => {
                            tracing::debug!("Accepted incoming client connection from {remote_addr:?}");

                            if let Err(e) = Self::handle_connection(socket, &mut rx).await {
                                tracing::error!("failed to handle conn: {e}");
                            }
                        }
                        Err(e) => tracing::warn!("Failed to accept incoming client connection: {e}"),
                    }
                }
                _ = shutdown_rx.recv() => {
                    tracing::info!("client notifier: received shutdown signal");
                    return Ok(());
                }
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
