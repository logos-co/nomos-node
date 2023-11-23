use std::{net::SocketAddr, time::Duration};

use serde::{Deserialize, Serialize};
use tokio::{
    net::TcpStream,
    sync::{mpsc, oneshot},
};

use crate::Body;

/// [`Connection`] is responsible for executing all [`ConnectionCommand`]s for a single specific
/// network connection.
/// If executing a [`ConnectionCommand`] is failed, the connection is closed,
/// the error is returned to the command requester,
/// and the next [`ConnectionCommand`] is executed with establishing a new connection.
pub(crate) struct Connection {
    tx: mpsc::UnboundedSender<ConnectionCommand>,
}

pub enum ConnectionCommand {
    /// Write a body to a connection, and return a result via channel.
    /// If failed, the body is returned along with an error,
    /// so that the caller can retry it if needed.
    Write {
        body: Body,
        tx: oneshot::Sender<core::result::Result<(), (Body, std::io::Error)>>,
    },
}

// TODO: add reconnect_max_backoff, so that the reconnect is not delayed for a long time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    reconnect_backoff: Duration,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            reconnect_backoff: Duration::from_secs(3),
        }
    }
}

impl Connection {
    pub fn new(config: ConnectionConfig, addr: SocketAddr) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        let mut runner = ConnectionRunner::new(config, addr, rx);
        tokio::spawn(async move {
            runner.run().await;
        });

        Self { tx }
    }

    pub fn submit(&mut self, cmd: ConnectionCommand) {
        self.tx.send(cmd).unwrap()
    }
}

struct ConnectionRunner {
    config: ConnectionConfig,
    addr: SocketAddr,
    rx: mpsc::UnboundedReceiver<ConnectionCommand>,
}

impl ConnectionRunner {
    fn new(
        config: ConnectionConfig,
        addr: SocketAddr,
        rx: mpsc::UnboundedReceiver<ConnectionCommand>,
    ) -> Self {
        Self { config, addr, rx }
    }

    /// Establish a connection on demand and reuse it for all [`ConnectionCommand`]s scheduled.
    /// If a command is failed to be executed, the connection is closed.
    /// The next command is executed with establishing a new connection.
    async fn run(&mut self) {
        let mut stream: Option<TcpStream> = None;
        let mut num_backoff = 0;

        while let Some(cmd) = self.rx.recv().await {
            self.connect_and_handle_command(cmd, &mut stream, &mut num_backoff)
                .await;
        }
    }

    async fn connect_and_handle_command(
        &mut self,
        cmd: ConnectionCommand,
        stream: &mut Option<TcpStream>,
        num_backoff: &mut i32,
    ) {
        // if connection has been never established or has been closed
        if stream.is_none() {
            Self::backoff(self.config.reconnect_backoff, *num_backoff).await;

            tracing::debug!("dialing to {}...", self.addr);
            match TcpStream::connect(self.addr).await {
                Ok(s) => {
                    tracing::debug!("connected to {} successfully", self.addr);
                    *stream = Some(s);
                    *num_backoff = 0;
                }
                Err(e) => {
                    tracing::error!("failed to connect to {}: {e}", self.addr);
                    Self::return_err(cmd, e).await;
                    *num_backoff += 1;
                    return;
                }
            }
        }

        if let Err(e) = Self::handle_command(cmd, stream.as_mut().unwrap()).await {
            tracing::error!("failed to handle command. closing connection: {e}");
            *stream = None;
            *num_backoff += 1;
        }
    }

    async fn handle_command(cmd: ConnectionCommand, stream: &mut TcpStream) -> std::io::Result<()> {
        match cmd {
            ConnectionCommand::Write { body, tx } => {
                tracing::trace!("writing body...");
                match body.write(stream).await {
                    Ok(_) => {
                        tx.send(Ok(())).unwrap();
                        Ok(())
                    }
                    Err(e) => {
                        let internal_err = std::io::Error::from(e.kind());
                        tx.send(Err((body, e))).unwrap();
                        Err(internal_err)
                    }
                }
            }
        }
    }

    async fn return_err(cmd: ConnectionCommand, e: std::io::Error) {
        match cmd {
            ConnectionCommand::Write { body, tx } => {
                tx.send(Err((body, e))).unwrap();
            }
        }
    }

    async fn backoff(reconnect_backoff: Duration, num_backoff: i32) {
        if num_backoff == 0 {
            return;
        }

        let backoff = Duration::from_secs(
            reconnect_backoff
                .as_secs()
                .pow(num_backoff.try_into().unwrap()),
        );
        tracing::trace!("backoff: sleeping {backoff:?}");
        tokio::time::sleep(backoff).await;
    }
}
