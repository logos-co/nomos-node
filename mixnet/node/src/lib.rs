mod client_notifier;
pub mod config;

use std::{
    collections::{HashMap, HashSet},
    error::Error,
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};

use client_notifier::ClientNotifier;
pub use config::MixnetNodeConfig;
use mixnet_protocol::{Body, PacketId};
use mixnet_topology::MixnetNodeId;
use mixnet_util::ConnectionPool;
use nym_sphinx::{
    addressing::nodes::NymNodeRoutingAddress, Delay, DestinationAddressBytes, NodeAddressBytes,
    Payload, PrivateKey,
};
pub use sphinx_packet::crypto::PRIVATE_KEY_SIZE;
use sphinx_packet::{crypto::PUBLIC_KEY_SIZE, ProcessedPacket, SphinxPacket};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{mpsc, Mutex},
};

// A mix node that routes packets in the Mixnet.
pub struct MixnetNode {
    config: MixnetNodeConfig,
    pool: ConnectionPool,
    ack_cache: Arc<Mutex<HashMap<SocketAddr, HashSet<PacketId>>>>,
}

impl MixnetNode {
    pub fn new(config: MixnetNodeConfig) -> Self {
        let pool = ConnectionPool::new(config.connection_pool_size);
        Self {
            config,
            pool,
            ack_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn id(&self) -> MixnetNodeId {
        self.public_key()
    }

    pub fn public_key(&self) -> [u8; PUBLIC_KEY_SIZE] {
        self.config.public_key()
    }

    const CLIENT_NOTI_CHANNEL_SIZE: usize = 100;

    pub async fn run(self) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
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
        tracing::info!(
            "Listening mixnet node connections: {}",
            self.config.listen_address
        );

        loop {
            match listener.accept().await {
                Ok((socket, remote_addr)) => {
                    tracing::debug!("Accepted incoming connection from {remote_addr:?}");

                    let client_tx = client_tx.clone();
                    let private_key = self.config.private_key;
                    let pool = self.pool.clone();
                    let ack_cache = self.ack_cache.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(
                            socket,
                            ack_cache,
                            self.config.max_retries,
                            self.config.retry_delay,
                            pool,
                            private_key,
                            client_tx,
                        )
                        .await
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
        ack_cache: Arc<Mutex<HashMap<SocketAddr, HashSet<PacketId>>>>,
        max_retries: usize,
        retry_delay: Duration,
        pool: ConnectionPool,
        private_key: [u8; PRIVATE_KEY_SIZE],
        client_tx: mpsc::Sender<Body>,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        loop {
            let body = Body::read(&mut socket).await?;

            let pool = pool.clone();
            let private_key = PrivateKey::from(private_key);
            let client_tx = client_tx.clone();
            let peer_addr = socket.peer_addr()?;
            let ack_cache = ack_cache.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::handle_body(
                    peer_addr,
                    ack_cache,
                    max_retries,
                    retry_delay,
                    body,
                    &pool,
                    &private_key,
                    &client_tx,
                )
                .await
                {
                    tracing::error!("failed to handle body: {e}");
                }
            });
        }
    }

    // TODO: refactor this fn to make it receive less arguments
    #[allow(clippy::too_many_arguments)]
    async fn handle_body(
        peer_addr: SocketAddr,
        ack_cache: Arc<Mutex<HashMap<SocketAddr, HashSet<PacketId>>>>,
        max_retries: usize,
        retry_delay: Duration,
        body: Body,
        pool: &ConnectionPool,
        private_key: &PrivateKey,
        client_tx: &mpsc::Sender<Body>,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        match body {
            Body::AckResponse(ack_response) => {
                tracing::debug!("Received ack response: {ack_response:?}");
                Self::handle_ack_response(peer_addr, ack_cache, ack_response).await
            }
            Body::SphinxPacket(packet) => {
                Self::handle_sphinx_packet(
                    pool,
                    ack_cache,
                    max_retries,
                    retry_delay,
                    private_key,
                    packet,
                )
                .await
            }
            Body::FinalPayload(payload) => {
                Self::forward_body_to_client_notifier(
                    private_key,
                    client_tx,
                    Body::FinalPayload(payload),
                )
                .await
            }
            _ => unreachable!(),
        }
    }

    async fn handle_ack_response(
        peer_addr: SocketAddr,
        ack_cache: Arc<Mutex<HashMap<SocketAddr, HashSet<PacketId>>>>,
        ack_response: mixnet_protocol::AckResponse,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut ack_cache = ack_cache.lock().await;
        if let Some(cache) = ack_cache.get_mut(&peer_addr) {
            if cache.remove(&ack_response.id) {
                tracing::debug!("Received ack response: {}", ack_response.id);
            } else {
                tracing::warn!(
                    "Received ack response with unknown packet id: {}",
                    ack_response.id
                );
            }
            Ok(())
        } else {
            tracing::warn!("Received ack response from unknown peer: {peer_addr}");
            Ok(())
        }
    }

    async fn handle_sphinx_packet(
        pool: &ConnectionPool,
        ack_cache: Arc<Mutex<HashMap<SocketAddr, HashSet<PacketId>>>>,
        max_retries: usize,
        retry_delay: Duration,
        private_key: &PrivateKey,
        packet: Box<SphinxPacket>,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        match packet.process(private_key)? {
            ProcessedPacket::ForwardHop(packet, next_node_addr, delay) => {
                Self::forward_packet_to_next_hop(
                    pool,
                    ack_cache,
                    max_retries,
                    retry_delay,
                    packet,
                    next_node_addr,
                    delay,
                )
                .await
            }
            ProcessedPacket::FinalHop(destination_addr, _, payload) => {
                Self::forward_payload_to_destination(
                    pool,
                    ack_cache,
                    max_retries,
                    retry_delay,
                    payload,
                    destination_addr,
                )
                .await
            }
        }
    }

    async fn forward_body_to_client_notifier(
        _private_key: &PrivateKey,
        client_tx: &mpsc::Sender<Body>,
        body: Body,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        // TODO: Decrypt the final payload using the private key, if it's encrypted

        // Do not wait when the channel is full or no receiver exists
        client_tx.try_send(body)?;
        Ok(())
    }

    async fn forward_packet_to_next_hop(
        pool: &ConnectionPool,
        ack_cache: Arc<Mutex<HashMap<SocketAddr, HashSet<PacketId>>>>,
        max_retries: usize,
        retry_delay: Duration,
        packet: Box<SphinxPacket>,
        next_node_addr: NodeAddressBytes,
        delay: Delay,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        tracing::debug!("Delaying the packet for {delay:?}");
        tokio::time::sleep(delay.to_duration()).await;

        Self::forward(
            pool,
            ack_cache,
            max_retries,
            retry_delay,
            Body::new_sphinx(packet),
            NymNodeRoutingAddress::try_from(next_node_addr)?,
        )
        .await
    }

    async fn forward_payload_to_destination(
        pool: &ConnectionPool,
        ack_cache: Arc<Mutex<HashMap<SocketAddr, HashSet<PacketId>>>>,
        max_retries: usize,
        retry_delay: Duration,
        payload: Payload,
        destination_addr: DestinationAddressBytes,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        tracing::debug!("Forwarding final payload to destination mixnode");

        Self::forward(
            pool,
            ack_cache,
            max_retries,
            retry_delay,
            Body::new_final_payload(payload),
            NymNodeRoutingAddress::try_from_bytes(&destination_addr.as_bytes())?,
        )
        .await
    }

    async fn forward(
        pool: &ConnectionPool,
        ack_cache: Arc<Mutex<HashMap<SocketAddr, HashSet<PacketId>>>>,
        max_retries: usize,
        retry_delay: Duration,
        body: Body,
        to: NymNodeRoutingAddress,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        let addr = SocketAddr::try_from(to)?;
        if let Some(data) = body
            .write(&mut *pool.get_or_init(&addr).await?.lock().await)
            .await?
        {
            let packet_id = PacketId::new(crc32fast::hash(&data));

            {
                let mut ack_cache = ack_cache.lock().await;
                ack_cache
                    .entry(addr)
                    .or_insert_with(HashSet::new)
                    .insert(packet_id);
            }

            let pool = pool.clone();
            tokio::spawn(async move {
                for _ in 0..max_retries {
                    tokio::time::sleep(retry_delay).await;
                    let ack_cache = ack_cache.lock().await;
                    if ack_cache.get(&addr).unwrap().contains(&packet_id) {
                        tracing::warn!("Retrying send a Sphinx packet to the node: {addr}");
                        let mu: std::sync::Arc<tokio::sync::Mutex<tokio::net::TcpStream>> =
                            pool.get_or_init(&addr).await.unwrap();
                        let mut socket = mu.lock().await;
                        if let Err(e) = Body::write_bytes(&mut *socket, 0, data.as_slice()).await {
                            tracing::error!("Failed to send a Sphinx packet: {e}");
                            // If we fail to send a Sphinx packet retry, try to update the pool
                            match TcpStream::connect(addr).await {
                                Ok(tcp) => {
                                    // If we can connect to the peer, let's update the pool
                                    *socket = tcp;
                                }
                                Err(e) => {
                                    tracing::error!("Peer is not connectable: {e}");
                                }
                            }
                        } else {
                            tracing::debug!(
                                "Sent a Sphinx packet successuflly to the node: {addr}"
                            );
                        }
                    } else {
                        return;
                    }
                }
            });
        }
        Ok(())
    }
}
