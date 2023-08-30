pub mod config;
mod receiver;
mod sender;

use std::error::Error;
use std::time::Duration;

pub use config::MixnetClientConfig;
pub use config::MixnetClientMode;
use futures::stream::BoxStream;
use mixnet_util::{ConnectionCache, MAX_OPEN_FILES_LIMIT};
use rand::Rng;
use sender::Sender;
use thiserror::Error;

// A client for sending packets to Mixnet and receiving packets from Mixnet.
pub struct MixnetClient<R: Rng> {
    mode: MixnetClientMode,
    sender: Sender<R>,
    cache: ConnectionCache,
}

pub type MessageStream = BoxStream<'static, Result<Vec<u8>, MixnetClientError>>;

impl<R: Rng> MixnetClient<R> {
    pub fn new(config: MixnetClientConfig, rng: R) -> Self {
        let cache = ConnectionCache::new(
            config
                .connection_cache_size
                .unwrap_or(MAX_OPEN_FILES_LIMIT.unwrap_or(u8::MAX as usize)),
        );
        Self {
            mode: config.mode,
            sender: Sender::new(config.topology, cache.clone(), rng),
            cache,
        }
    }

    pub async fn run(&self) -> Result<MessageStream, MixnetClientError> {
        self.mode.run(self.cache.clone()).await
    }

    pub fn send(&mut self, msg: Vec<u8>, total_delay: Duration) -> Result<(), Box<dyn Error>> {
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
