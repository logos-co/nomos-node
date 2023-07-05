use std::error::Error;

use libp2p::PeerId;
use multiaddr::Multiaddr;
use tokio::sync::oneshot;

/// Commands that users can execute to [`NomosLibp2p`].
/// The result can be received from the [`result_tx`].
pub struct Command {
    pub message: CommandMessage,
    pub result_tx: CommandResultSender,
}

#[derive(Debug)]
pub enum CommandMessage {
    Connect(PeerId, Multiaddr),
    Broadcast { topic: Topic, message: Vec<u8> },
    Subscribe(Topic),
    Unsubscribe(Topic),
}

pub type Topic = String;
pub type CommandResultSender = oneshot::Sender<Result<(), Box<dyn Error + Send>>>;
