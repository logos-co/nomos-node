pub mod config;
mod receiver;
mod sender;

use std::error::Error;

pub use config::MixnetClientConfig;
pub use config::MixnetClientMode;
use futures::Sink;
use rand::Rng;
use sender::Sender;

// A client for sending packets to Mixnet and receiving packets from Mixnet.
pub struct MixnetClient<R: Rng> {
    mode: MixnetClientMode,
    sender: Sender<R>,
}

impl<R: Rng> MixnetClient<R> {
    pub fn new(config: MixnetClientConfig, rng: R) -> Self {
        Self {
            mode: config.mode,
            sender: Sender::new(config.topology, rng),
        }
    }

    // TODO: Instead of providing this sync function that spawns a task internally,
    // it would be better to provide the `Stream` trait implementation
    // for better composability from the user's point of view.
    // https://github.com/logos-co/nomos-node/issues/314
    pub fn run(&self, message_tx: impl Sink<Vec<u8>> + Clone + Unpin + Send + Sync + 'static) {
        let mode = self.mode.clone();
        tokio::spawn(async move {
            if let Err(e) = mode.run(message_tx).await {
                tracing::error!("error from mixnet client receiver: {e}");
            }
        });
    }

    pub fn send(&mut self, msg: Vec<u8>) -> Result<(), Box<dyn Error>> {
        self.sender.send(msg)
    }
}
