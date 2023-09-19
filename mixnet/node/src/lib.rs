mod client_notifier;
pub mod config;

use std::{net::SocketAddr, time::Duration};

use client_notifier::ClientNotifier;
pub use config::MixnetNodeConfig;
use mixnet_protocol::{Body, ProtocolError};
use mixnet_topology::MixnetNodeId;
use mixnet_util::{MessageHandle, MessagePool, OneOrMore, RetryPool};
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
    SendError(#[from] tokio::sync::mpsc::error::TrySendError<Body>),
    #[error("client: {0}")]
    Client(ProtocolError),
}

// A mix node that routes packets in the Mixnet.
pub struct MixnetNode {
    config: MixnetNodeConfig,
    message_pool: MessagePool<Message>,
}

impl MixnetNode {
    pub fn new(config: MixnetNodeConfig) -> Self {
        Self {
            config,
            message_pool: MessagePool::new(),
        }
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
        //TODO: Add graceful shutdown
        let listener = TcpListener::bind(self.config.listen_address)
            .await
            .map_err(ProtocolError::IO)?;
        tracing::info!(
            "Listening mixnet node connections: {}",
            self.config.listen_address
        );

        let runner = MixnetNodeRunner {
            config: self.config,
            message_pool: self.message_pool,
            client_tx,
        };

        loop {
            match listener.accept().await {
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
    }
}

struct Message {
    // number of retries already done
    // TODO: remove this allow when we implement the retry logic
    #[allow(dead_code)]
    retries: usize,
    body: Body,
}

#[derive(Clone)]
struct MixnetNodeRunner {
    config: MixnetNodeConfig,
    message_pool: MessagePool<Message>,
    client_tx: mpsc::Sender<Body>,
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

    async fn handle_body(&self, msg: Body) -> Result<()> {
        match msg {
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
                self.forward_packet_to_next_hop(
                    Message {
                        retries: 0,
                        body: Body::SphinxPacket(packet),
                    },
                    next_node_addr,
                    delay,
                )
                .await
            }
            ProcessedPacket::FinalHop(destination_addr, _, payload) => {
                self.forward_payload_to_destination(
                    Message {
                        retries: 0,
                        body: Body::FinalPayload(payload),
                    },
                    destination_addr,
                )
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
        packet: Message,
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
        payload: Message,
        destination_addr: DestinationAddressBytes,
    ) -> Result<()> {
        tracing::debug!("Forwarding final payload to destination mixnode");

        self.forward(
            payload,
            NymNodeRoutingAddress::try_from_bytes(&destination_addr.as_bytes())?,
        )
        .await
    }

    async fn forward(&self, msg: Message, to: NymNodeRoutingAddress) -> Result<()> {
        let addr = SocketAddr::from(to);

        let body = if let Some(handle) = self.message_pool.get(&addr) {
            if let Err(err) = handle.send(msg) {
                err.0
            } else {
                // If succesfull send message to msg writting thread,
                // then we just return
                return Ok(());
            }
        } else {
            msg
        };

        // the target msg handling thread does not live,
        // so we need to create a new one
        let (tx, rx) = mpsc::unbounded_channel();
        tx.send(body).unwrap();

        let this = MessageHandler::new(
            addr,
            // TODO: do we need to retry on error for the tcp stream here?
            TcpStream::connect(addr)
                .await
                .map_err(|e| MixnetNodeError::Protocol(ProtocolError::IO(e)))?,
            rx,
            self.config,
        );

        let handle = tokio::spawn(async move {
            this.run().await;
        });
        self.message_pool
            .insert(addr, MessageHandle::new(tx, handle));
        Ok(())
    }
}

struct MessageHandler {
    // TODO: remove this allow when we implement the retry logic
    #[allow(dead_code)]
    config: MixnetNodeConfig,
    remote_addr: SocketAddr,
    receiver: mpsc::UnboundedReceiver<Message>,
    retry_pool: RetryPool<Message>,
    conn: TcpStream,
}

impl MessageHandler {
    pub fn new(
        remote_addr: SocketAddr,
        conn: TcpStream,
        receiver: mpsc::UnboundedReceiver<Message>,
        config: MixnetNodeConfig,
    ) -> Self {
        Self {
            receiver,
            remote_addr,
            retry_pool: RetryPool::new(),
            conn,
            config,
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                msg = self.receiver.recv() => {
                    if let Some(msg) = msg {
                        match self.send(msg, None).await {
                            Ok(Some((delay, msg))) => {
                                self.retry_pool.insert(msg, delay).await;
                            }
                            Ok(None) => {},
                            Err(e) => return,
                        }
                    } else {
                        // Channel closed, we should shutdown the message handler thread
                        return;
                    }
                },
                retry_msg = async {
                    let fut = self.retry_pool.get().await;
                    fut.await
                } => {
                    match retry_msg {
                        OneOrMore::One((last_delay, msg)) => {
                            match self.send(msg, Some(last_delay)).await {
                                Ok(Some((delay, msg))) => {
                                    self.retry_pool.insert(msg, delay).await;
                                }
                                Ok(None) => {},
                                Err(e) => return,
                            }
                        },
                        OneOrMore::More(msgs) => {
                            match self.send_many(msgs).await {
                                Ok(Some(msgs)) => {
                                    self.retry_pool.insert_many(msgs.into_iter()).await;
                                }
                                Ok(None) => {},
                                Err(e) => return,
                            }
                        },
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("Shutting down message handler thread for {}...", self.remote_addr);
                    return;
                }
            }
        }
    }

    /// Send a message to the remote node,
    /// return Ok(Some(Vec<(Duration, Message)>)) if the message is not sent and the error is retryable
    /// return Ok(None) if the message is sent successfully
    /// return Err(e) if the message is not sent and the error is not retryable
    async fn send_many(
        &mut self,
        msgs: Vec<(Duration, Message)>,
    ) -> Result<Option<Vec<(Duration, Message)>>> {
        let mut retries = Vec::new();
        for (delay, msg) in msgs {
            match self.send(msg, Some(delay)).await {
                Ok(None) => continue,
                Ok(Some((d, msg))) => retries.push((d, msg)),
                Err(e) => return Err(e),
            }
        }

        Ok(Some(retries))
    }

    /// Send a message to the remote node,
    /// return Ok(Some((Duration, Message))) if the message is not sent and the error is retryable
    /// return Ok(None) if the message is sent successfully
    /// return Err(e) if the message is not sent and the error is not retryable
    async fn send(
        &mut self,
        mut msg: Message,
        last_delay: Option<Duration>,
    ) -> Result<Option<(Duration, Message)>> {
        use std::io::ErrorKind;

        match msg.body.write(&mut self.conn).await {
            Ok(_) => Ok(None),
            // we should only retry io errors (exclude unsupported for now, may be more in future)
            Err(ProtocolError::IO(e)) if e.kind() != ErrorKind::Unsupported => {
                // Update the connection, I actully do not want to do it unless the connection is in broken
                // situation, but rust does not provide a method to let us check if the connection is broken
                // or not, so I just hard code the possible situations worth to refresh.
                if matches!(
                    e.kind(),
                    ErrorKind::ConnectionAborted
                        | ErrorKind::ConnectionReset
                        | ErrorKind::ConnectionRefused
                        | ErrorKind::NotConnected
                        | ErrorKind::BrokenPipe
                        | ErrorKind::TimedOut
                ) {
                    match TcpStream::connect(self.remote_addr).await {
                        Ok(tcp) => {
                            self.conn = tcp;
                        }
                        Err(e) => {
                            tracing::error!("failed to update connection to {}, the local machine network is down: {e}", self.remote_addr);
                            return Err(MixnetNodeError::Protocol(ProtocolError::IO(e)));
                        }
                    }
                }

                if msg.retries < self.config.max_retries {
                    msg.retries += 1;
                    let delay = match last_delay {
                        // exponential backoff
                        Some(last_delay) => Duration::from_millis(
                            (last_delay.as_millis() as u64).pow(msg.retries as u32),
                        ),
                        None => self.config.retry_delay,
                    };
                    return Ok(Some((delay, msg)));
                }

                tracing::error!(
                    "failed to forward msg to {}: reach the maximum retries",
                    self.remote_addr
                );
                Err(MixnetNodeError::Protocol(ProtocolError::ReachMaxRetries(
                    self.config.max_retries,
                )))
            }
            Err(e) => {
                tracing::error!("failed to forward msg to {}: {e}", self.remote_addr);
                Err(MixnetNodeError::Protocol(e))
            }
        }
    }
}
