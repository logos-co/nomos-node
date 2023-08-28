mod client_notifier;
pub mod config;

use std::{error::Error, net::SocketAddr, sync::Arc};

use atomic::Atomic;
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

pin_project_lite::pin_project! {
    pub struct MixnetNodeHandle {
        listen_address: SocketAddr,
        client_listen_address: Arc<Atomic<SocketAddr>>,
        private_key: [u8; PRIVATE_KEY_SIZE],
        shutdown_tx: async_channel::Sender<()>,
        #[pin]
        handle: tokio::task::JoinHandle<()>,
    }
}

impl MixnetNodeHandle {
    /// Returns the mix node listening address.
    pub fn listen_address(&self) -> SocketAddr {
        self.listen_address
    }

    /// Returns the actual client listening address.
    pub fn client_listen_address(&self) -> SocketAddr {
        self.client_listen_address.load(atomic::Ordering::Acquire)
    }

    /// Returns the private key of the mix node.
    pub fn private_key(&self) -> [u8; PRIVATE_KEY_SIZE] {
        self.private_key
    }

    /// Shutdown the mixnet node, it is safe to call this function multiple times.
    /// Returns `true` if this call has shutdown the mix node and it was not shutdown already
    pub fn shutdown(&self) -> bool {
        self.shutdown_tx.close()
    }
}

impl std::future::Future for MixnetNodeHandle {
    type Output = Result<(), tokio::task::JoinError>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.project();
        match this.handle.poll(cx) {
            std::task::Poll::Ready(rst) => {
                this.shutdown_tx.close();
                std::task::Poll::Ready(rst)
            }
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
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

    pub async fn run(mut self) -> Result<MixnetNodeHandle, Box<dyn Error>> {
        let (shutdown_tx, shutdown_rx) = async_channel::bounded(1);
        // Spawn a ClientNotifier
        let (client_tx, client_rx) = mpsc::channel(Self::CLIENT_NOTI_CHANNEL_SIZE);
        let client_listen_address = Arc::new(Atomic::new(self.config.client_listen_address));
        let notifier_listen_address = client_listen_address.clone();
        let notifier_shutdown_rx = shutdown_rx.clone();
        tokio::spawn(async move {
            if let Err(e) =
                ClientNotifier::run(notifier_listen_address, client_rx, notifier_shutdown_rx).await
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

        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    socket = listener.accept() => {
                        match socket {
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
                    _ = shutdown_rx.recv() => {
                        tracing::info!("mixnet node: received shutdown signal");
                        return;
                    }
                }
            }
        });

        Ok(MixnetNodeHandle {
            listen_address: self.config.listen_address,
            client_listen_address,
            private_key: self.config.private_key,
            shutdown_tx,
            handle,
        })
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
