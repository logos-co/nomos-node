use std::error::Error;

use libp2p::PeerId;
use multiaddr::Multiaddr;
use tokio::sync::oneshot;

pub struct Command {
    pub message: CommandMessage,
    pub sender: CommandSender,
}

#[derive(Debug)]
pub enum CommandMessage {
    Connect(PeerId, Multiaddr),
    Broadcast { topic: Topic, message: Vec<u8> },
    Subscribe(Topic),
    Unsubscribe(Topic),
}

pub type Topic = String;
pub type CommandSender = oneshot::Sender<Result<(), Box<dyn Error + Send>>>;
