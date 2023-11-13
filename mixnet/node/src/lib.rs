mod client_notifier;
pub mod config;

use std::{
    collections::{hash_map::Entry, HashMap},
    net::SocketAddr,
};

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

        let packet_forwarder = ForwardScheduler::new(rx, self.config);

        tokio::spawn(async move {
            packet_forwarder.run().await;
        });

        let inbound_handler = InboundHandler {
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

#[derive(Clone)]
struct InboundHandler {
    config: MixnetNodeConfig,
    client_tx: mpsc::Sender<Body>,
    packet_tx: mpsc::UnboundedSender<Packet>,
}

impl InboundHandler {
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

    async fn handle_body(&self, body: Body) -> Result<()> {
        match body {
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

    async fn forward(&self, body: Body, to: NymNodeRoutingAddress) -> Result<()> {
        let addr = SocketAddr::from(to);

        self.packet_tx.send(Packet::new(addr, body))?;
        Ok(())
    }
}

struct ForwardScheduler {
    config: MixnetNodeConfig,
    rx: mpsc::UnboundedReceiver<Packet>,
    forwarders: HashMap<SocketAddr, Forwarder>,
}

impl ForwardScheduler {
    pub fn new(rx: mpsc::UnboundedReceiver<Packet>, config: MixnetNodeConfig) -> Self {
        Self {
            config,
            rx,
            forwarders: HashMap::with_capacity(config.connection_pool_size),
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                packet = self.rx.recv() => {
                    if let Some(packet) = packet {
                        self.schedule(packet, self.config).await;
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

    async fn schedule(&mut self, packet: Packet, config: MixnetNodeConfig) {
        if let Entry::Vacant(entry) = self.forwarders.entry(packet.target) {
            entry.insert(Forwarder::new(packet.target, config));
        }

        let forwarder = self.forwarders.get_mut(&packet.target).unwrap();
        forwarder.schedule(packet.body);
    }
}

struct Forwarder {
    tx: mpsc::UnboundedSender<Body>,
}

impl Forwarder {
    fn new(addr: SocketAddr, config: MixnetNodeConfig) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        let tx_ = tx.clone();
        tokio::spawn(async move {
            Forwarder::run(addr, rx, tx_, config).await;
        });

        Self { tx }
    }

    async fn run(
        addr: SocketAddr,
        mut rx: mpsc::UnboundedReceiver<Body>,
        mut tx: mpsc::UnboundedSender<Body>,
        config: MixnetNodeConfig,
    ) {
        loop {
            match Forwarder::connect_and_forward(addr, &mut rx, &mut tx).await {
                Ok(_) => {
                    tracing::debug!("closing the forwarder: {addr}");
                    return;
                }
                Err(e) => {
                    tracing::error!("retrying: failed to connect and forward to {addr}: {e}");
                    tokio::time::sleep(config.retry_delay).await;
                }
            }
        }
    }

    async fn connect_and_forward(
        addr: SocketAddr,
        rx: &mut mpsc::UnboundedReceiver<Body>,
        tx: &mut mpsc::UnboundedSender<Body>,
    ) -> core::result::Result<(), ProtocolError> {
        match TcpStream::connect(addr).await {
            Ok(mut conn) => {
                while let Some(body) = rx.recv().await {
                    if let Err(e) = body.write(&mut conn).await {
                        match e {
                            ProtocolError::IO(_) => {
                                tx.send(body).expect("the receiver half is always open");
                                return Err(e);
                            }
                            _ => {
                                tracing::error!("ignoring: failed to forward body to {addr}: {e}",);
                            }
                        }
                    } else {
                        tracing::debug!("forwarded body to {addr}");
                    }
                }

                // the sender half has been closed.
                Ok(())
            }
            Err(e) => Err(ProtocolError::from(e)),
        }
    }

    pub fn schedule(&mut self, body: Body) {
        self.tx
            .send(body)
            .expect("the receiver half is always open");
    }
}

pub struct Packet {
    target: SocketAddr,
    body: Body,
}

impl Packet {
    fn new(target: SocketAddr, body: Body) -> Self {
        Self { target, body }
    }
}
