pub mod config;
pub mod error;
pub use error::*;
use mixnet_protocol::connection::ConnectionPool;
mod receiver;
mod sender;

use std::time::Duration;

pub use config::MixnetClientConfig;
pub use config::MixnetClientMode;
use futures::stream::BoxStream;
use rand::Rng;
use sender::Sender;

// A client for sending packets to Mixnet and receiving packets from Mixnet.
pub struct MixnetClient<R: Rng> {
    mode: MixnetClientMode,
    sender: Sender<R>,
}

pub type MessageStream = BoxStream<'static, Result<Vec<u8>>>;

impl<R: Rng> MixnetClient<R> {
    pub fn new(config: MixnetClientConfig, rng: R) -> Self {
        Self {
            mode: config.mode,
            sender: Sender::new(
                config.topology,
                ConnectionPool::new(config.connection_pool_config),
                rng,
                config.max_net_write_tries,
            ),
        }
    }

    pub async fn run(&self) -> Result<MessageStream> {
        self.mode.run().await
    }

    pub fn send(&mut self, msg: Vec<u8>, total_delay: Duration) -> Result<()> {
        self.sender.send(msg, total_delay)
    }
}
