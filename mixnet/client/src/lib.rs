pub mod config;
mod receiver;
mod sender;

use std::error::Error;

pub use config::MixnetClientConfig;
pub use config::MixnetClientMode;
use rand::Rng;
use sender::Sender;
use tokio::sync::broadcast;

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

    pub fn run(&self) -> Option<broadcast::Receiver<Vec<u8>>> {
        self.mode.run()
    }

    pub fn send(&mut self, msg: Vec<u8>) -> Result<(), Box<dyn Error>> {
        self.sender.send(msg)
    }
}
