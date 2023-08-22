pub mod config;
mod receiver;
mod sender;

use std::error::Error;

pub use config::MixnetClientConfig;
pub use config::MixnetClientMode;
use futures::Stream;
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

    pub async fn run(
        &self,
    ) -> Result<impl Stream<Item = Result<Vec<u8>, Box<dyn Error>>> + Send + 'static, Box<dyn Error>>
    {
        let mode = self.mode.clone();
        mode.run()
            .await
            .map(|stream| stream.expect("Expected client mode"))
    }

    pub fn send(&mut self, msg: Vec<u8>) -> Result<(), Box<dyn Error>> {
        self.sender.send(msg)
    }
}
