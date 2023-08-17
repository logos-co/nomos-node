pub mod config;
mod receiver;
mod sender;

use std::error::Error;

pub use config::MixnetClientConfig;
use rand::Rng;
use receiver::Receiver;
use sender::Sender;
use tokio::sync::broadcast;

// A client for sending packets to Mixnet and receiving packets from Mixnet.
pub struct MixnetClient {
    sender: Sender,
    message_tx: broadcast::Sender<Vec<u8>>,
}

const CHANNEL_SIZE: usize = 100;

impl MixnetClient {
    pub async fn run(config: MixnetClientConfig) -> Result<Self, Box<dyn Error>> {
        let (message_tx, _) = broadcast::channel(CHANNEL_SIZE);

        // Run a receiver only if listen_address is specified
        if let Some(listen_address) = config.listen_address {
            Receiver::run(listen_address, message_tx.clone()).await?;
        }

        Ok(Self {
            sender: Sender::new(config.topology),
            message_tx,
        })
    }

    pub fn send<R: Rng>(&self, msg: Vec<u8>, rng: &mut R) -> Result<(), Box<dyn Error>> {
        self.sender.send(msg, rng)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.message_tx.subscribe()
    }
}
