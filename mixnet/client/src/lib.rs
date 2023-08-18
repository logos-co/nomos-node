pub mod config;
mod receiver;
mod sender;

use std::{error::Error, net::SocketAddr};

pub use config::MixnetClientConfig;
pub use config::MixnetClientMode;
use rand::Rng;
use receiver::Receiver;
use sender::Sender;
use tokio::sync::broadcast;

// A client for sending packets to Mixnet and receiving packets from Mixnet.
pub trait MixnetClient {
    fn send(&mut self, msg: Vec<u8>) -> Result<(), Box<dyn Error>>;
    fn subscribe(&self) -> Result<broadcast::Receiver<Vec<u8>>, Box<dyn Error>>;
}

pub fn new<R: Rng + 'static>(config: MixnetClientConfig, rng: R) -> Box<dyn MixnetClient> {
    match config.mode {
        MixnetClientMode::Sender => Box::new(MixnetClientSender::new(config, rng)),
        MixnetClientMode::SenderReceiver(listen_address) => {
            Box::new(MixnetClientSenderReceiver::new(config, listen_address, rng))
        }
    }
}

// A client with the sender mode
struct MixnetClientSender<R: Rng> {
    sender: Sender<R>,
}

impl<R: Rng> MixnetClientSender<R> {
    fn new(config: MixnetClientConfig, rng: R) -> Self {
        Self {
            sender: Sender::new(config.topology, rng),
        }
    }
}

impl<R: Rng> MixnetClient for MixnetClientSender<R> {
    fn send(&mut self, msg: Vec<u8>) -> Result<(), Box<dyn Error>> {
        self.sender.send(msg)
    }

    fn subscribe(&self) -> Result<broadcast::Receiver<Vec<u8>>, Box<dyn Error>> {
        Err("subscribe is not supported in sender mode".into())
    }
}

struct MixnetClientSenderReceiver<R: Rng> {
    sender: Sender<R>,
    message_tx: broadcast::Sender<Vec<u8>>,
}

// A client with the sender-receiver mode
impl<R: Rng> MixnetClientSenderReceiver<R> {
    const CHANNEL_SIZE: usize = 100;

    fn new(config: MixnetClientConfig, listen_address: SocketAddr, rng: R) -> Self {
        let (message_tx, _) = broadcast::channel(Self::CHANNEL_SIZE);

        let tx = message_tx.clone();
        tokio::spawn(async move { Receiver::run(listen_address, tx).await.unwrap() });

        Self {
            sender: Sender::new(config.topology, rng),
            message_tx,
        }
    }
}

impl<R: Rng> MixnetClient for MixnetClientSenderReceiver<R> {
    fn send(&mut self, msg: Vec<u8>) -> Result<(), Box<dyn Error>> {
        self.sender.send(msg)
    }

    fn subscribe(&self) -> Result<broadcast::Receiver<Vec<u8>>, Box<dyn Error>> {
        Ok(self.message_tx.subscribe())
    }
}
