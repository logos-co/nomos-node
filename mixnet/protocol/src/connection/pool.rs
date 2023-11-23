use std::{collections::HashMap, net::SocketAddr};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::{
    conn::{Connection, ConnectionCommand},
    ConnectionConfig,
};

#[derive(Clone)]
pub struct ConnectionPool {
    tx: mpsc::UnboundedSender<Command>,
}

pub struct Command {
    pub addr: SocketAddr,
    pub command: ConnectionCommand,
}

// TODO: add capacity and eviction
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConnectionPoolConfig {
    pub conn_config: ConnectionConfig,
}

/// [`ConnectionPool`] executes a task per connection, and forwards [`ConnectionCommand`]s to
/// the task that is responsible for the corresponding [`Connection`].
impl ConnectionPool {
    pub fn new(config: ConnectionPoolConfig) -> ConnectionPool {
        let (tx, rx) = mpsc::unbounded_channel();

        let runner = ConnectionPoolRunner::new(rx, config);
        tokio::spawn(async move { runner.run().await });

        Self { tx }
    }

    pub fn submit(&mut self, cmd: Command) {
        self.tx.send(cmd).unwrap();
    }
}

struct ConnectionPoolRunner {
    config: ConnectionPoolConfig,
    rx: mpsc::UnboundedReceiver<Command>,
    connections: HashMap<SocketAddr, Connection>,
}

impl ConnectionPoolRunner {
    fn new(rx: mpsc::UnboundedReceiver<Command>, config: ConnectionPoolConfig) -> Self {
        Self {
            config,
            rx,
            connections: Default::default(),
        }
    }

    async fn run(mut self) {
        while let Some(cmd) = self.rx.recv().await {
            self.handle_cmd(cmd).await;
        }
    }

    // NOTE: Since Runner is a single task who handles all connections,
    // each command handling must be very light to avoid the HOL blocking.
    async fn handle_cmd(&mut self, cmd: Command) {
        self.connections
            .entry(cmd.addr)
            .or_insert_with(|| Connection::new(self.config.conn_config.clone(), cmd.addr))
            .submit(cmd.command);
    }
}
