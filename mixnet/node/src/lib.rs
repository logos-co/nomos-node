mod client_notifier;
pub mod config;

use std::{collections::HashMap, net::SocketAddr, time::Duration};

use client_notifier::ClientNotifier;
pub use config::MixnetNodeConfig;
use mixnet_protocol::{Body, ProtocolError};
use mixnet_topology::MixnetNodeId;
use nym_sphinx::{
    addressing::nodes::{NymNodeRoutingAddress, NymNodeRoutingAddressError},
    Delay, DestinationAddressBytes, NodeAddressBytes, PrivateKey,
};
pub use sphinx_packet::crypto::PRIVATE_KEY_SIZE;
use sphinx_packet::{crypto::PUBLIC_KEY_SIZE, ProcessedPacket, SphinxPacket};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
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

        let packet_forwarder = PacketForwarder::new(tx.clone(), rx, self.config);

        tokio::spawn(async move {
            packet_forwarder.run().await;
        });

        let runner = MixnetNodeRunner {
            config: self.config,
            client_tx,
            packet_tx: tx,
        };

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
    packet_tx: mpsc::UnboundedSender<Packet>,
}

impl MixnetNodeRunner {
    async fn handle_connection(&self, mut socket: TcpStream) -> Result<()> {
        loop {
            let body = Body::read(&mut socket).await?;
            let this = self.clone();
            tokio::spawn(async move {
                if let Err(e) = this.handle_body(body).await {
                    tracing::error!("failed to handle body: {e}");
                }
            });
        }
    }

    async fn handle_body(&self, pkt: Body) -> Result<()> {
        match pkt {
            Body::SphinxPacket(packet) => self.handle_sphinx_packet(packet).await,
            Body::FinalPayload(payload) => {
                self.forward_body_to_client_notifier(Body::FinalPayload(payload))
                    .await
            }
            _ => unreachable!(),
        }
    }

    async fn handle_sphinx_packet(&self, packet: Box<SphinxPacket>) -> Result<()> {
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
        &self,
        packet: Body,
        next_node_addr: NodeAddressBytes,
        delay: Delay,
    ) -> Result<()> {
        tracing::debug!("Delaying the packet for {delay:?}");
        tokio::time::sleep(delay.to_duration()).await;

        self.forward(packet, NymNodeRoutingAddress::try_from(next_node_addr)?)
            .await
    }

    async fn forward_payload_to_destination(
        &self,
        payload: Body,
        destination_addr: DestinationAddressBytes,
    ) -> Result<()> {
        tracing::debug!("Forwarding final payload to destination mixnode");

        self.forward(
            payload,
            NymNodeRoutingAddress::try_from_bytes(&destination_addr.as_bytes())?,
        )
        .await
    }

    async fn forward(&self, pkt: Body, to: NymNodeRoutingAddress) -> Result<()> {
        let addr = SocketAddr::from(to);

        self.packet_tx.send(Packet::new(addr, pkt))?;
        Ok(())
    }
}

struct PacketForwarder {
    config: MixnetNodeConfig,
    packet_rx: mpsc::UnboundedReceiver<Packet>,
    packet_tx: mpsc::UnboundedSender<Packet>,
    connections: HashMap<SocketAddr, TcpStream>,
}

impl PacketForwarder {
    pub fn new(
        packet_tx: mpsc::UnboundedSender<Packet>,
        packet_rx: mpsc::UnboundedReceiver<Packet>,
        config: MixnetNodeConfig,
    ) -> Self {
        Self {
            packet_tx,
            packet_rx,
            connections: HashMap::with_capacity(config.connection_pool_size),
            config,
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                pkt = self.packet_rx.recv() => {
                    if let Some(pkt) = pkt {
                        self.send(pkt).await;
                    } else {
                        unreachable!("Packet channel should not be closed, because PacketForwarder is also holding the send half");
                    }
                },
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("Shutting down packet forwarder task...");
                    return;
                }
            }
        }
    }

    async fn try_send(&mut self, target: SocketAddr, body: &Body) -> Result<()> {
        if let std::collections::hash_map::Entry::Vacant(e) = self.connections.entry(target) {
            match TcpStream::connect(target).await {
                Ok(tcp) => {
                    e.insert(tcp);
                }
                Err(e) => {
                    tracing::error!("failed to connect to {}: {e}", target);
                    return Err(MixnetNodeError::Protocol(e.into()));
                }
            }
        }
        Ok(body
            .write(self.connections.get_mut(&target).unwrap())
            .await?)
    }

    async fn send(&mut self, pkt: Packet) {
        if let Err(err) = self.try_send(pkt.target, &pkt.body).await {
            match err {
                MixnetNodeError::Protocol(ProtocolError::IO(e))
                    if e.kind() == std::io::ErrorKind::Unsupported =>
                {
                    tracing::error!("fail to send message to {}: {e}", pkt.target);
                }
                _ => self.handle_retry(pkt),
            }
        }
    }

    fn handle_retry(&self, mut pkt: Packet) {
        if pkt.retry_count < self.config.max_retries {
            let delay = Duration::from_millis(
                (self.config.retry_delay.as_millis() as u64).pow(pkt.retry_count as u32),
            );
            let tx = self.packet_tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(delay).await;
                pkt.retry_count += 1;
                if let Err(e) = tx.send(pkt) {
                    tracing::error!("fail to enqueue retry message: {e}");
                }
            });
        } else {
            tracing::error!(
                "fail to send message to {}: reach maximum retries",
                pkt.target
            );
        }
    }
}

pub struct Packet {
    target: SocketAddr,
    body: Body,
    retry_count: usize,
}

impl Packet {
    fn new(target: SocketAddr, body: Body) -> Self {
        Self {
            target,
            body,
            retry_count: 0,
        }
    }
}
