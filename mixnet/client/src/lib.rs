pub mod config;
mod receiver;
mod sender;

use std::error::Error;
use std::time::Duration;

pub use config::MixnetClientConfig;
pub use config::MixnetClientMode;
use futures::stream::BoxStream;
use mixnet_util::ConnectionPool;
use rand::Rng;
use sender::Sender;
use thiserror::Error;

// A client for sending packets to Mixnet and receiving packets from Mixnet.
pub struct MixnetClient<R: Rng> {
    mode: MixnetClientMode,
    sender: Sender<R>,
}

pub type MessageStream = BoxStream<'static, Result<Vec<u8>, MixnetClientError>>;

impl<R: Rng> MixnetClient<R> {
    pub fn new(config: MixnetClientConfig, rng: R) -> Self {
        let cache = ConnectionPool::new(config.connection_pool_size);
        Self {
            mode: config.mode,
            sender: Sender::new(
                config.topology,
                cache,
                rng,
                config.max_retries,
                config.retry_delay,
            ),
        }
    }

    pub async fn run(&self) -> Result<MessageStream, MixnetClientError> {
        self.mode.run().await
    }

    pub fn send(
        &mut self,
        msg: Vec<u8>,
        total_delay: Duration,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        self.sender.send(msg, total_delay)
    }
}

#[derive(Error, Debug)]
pub enum MixnetClientError {
    #[error("mixnet node connect error")]
    MixnetNodeConnectError,
    #[error("mixnode stream has been closed")]
    MixnetNodeStreamClosed,
    #[error("unexpected stream body received")]
    UnexpectedStreamBody,
    #[error("invalid payload")]
    InvalidPayload,
}
