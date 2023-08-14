pub mod config;
mod receiver;
mod sender;

use std::{error::Error, net::SocketAddr};

use config::Config;
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
    pub async fn run(config: Config) -> Result<Self, Box<dyn Error>> {
        let (message_tx, _) = broadcast::channel(CHANNEL_SIZE);

        Receiver::run(config.listen_addr, message_tx.clone()).await?;

        Ok(Self {
            sender: Sender::new(config.topology),
            message_tx,
        })
    }

    pub fn send(
        &self,
        msg: Vec<u8>,
        destination: SocketAddr,
        num_hops: usize,
    ) -> Result<(), Box<dyn Error>> {
        self.sender.send(msg, destination, num_hops)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.message_tx.subscribe()
    }
}
