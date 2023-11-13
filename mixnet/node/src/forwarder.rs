use std::net::SocketAddr;

use mixnet_protocol::{Body, ProtocolError};
use tokio::{net::TcpStream, sync::mpsc};

use crate::MixnetNodeConfig;

/// [`Forwarder`] holds a TCP connection to another mix node,
/// and forwards all incoming packets to it.
pub struct Forwarder {
    tx: mpsc::UnboundedSender<Body>,
}

impl Forwarder {
    pub fn new(addr: SocketAddr, config: MixnetNodeConfig) -> Self {
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
