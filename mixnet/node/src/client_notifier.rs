use std::{error::Error, net::SocketAddr, sync::Arc};

use atomic::{Atomic, Ordering};
use mixnet_protocol::Body;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

pub struct ClientNotifier {}

impl ClientNotifier {
    pub async fn run(
        listen_address: Arc<Atomic<SocketAddr>>,
        mut rx: mpsc::Receiver<Body>,
        shutdown_rx: async_channel::Receiver<()>,
    ) -> Result<(), Box<dyn Error>> {
        let mut addr = listen_address.load(Ordering::Acquire);
        let listener = TcpListener::bind(addr).await?;

        // update the port if the port is assigned automatically by the system
        if addr.port() == 0 {
            addr.set_port(listener.local_addr().unwrap().port());
            listen_address.store(addr, Ordering::Release);
        }

        tracing::info!("Listening mixnet client connections: {addr}");

        // Currently, handling only a single incoming connection
        // TODO: consider handling multiple clients
        loop {
            tokio::select! {
                socket = listener.accept() => {
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
