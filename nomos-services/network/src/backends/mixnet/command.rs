use nomos_core::wire;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::backends::libp2p::{Libp2pInfo, Topic};

#[derive(Debug)]
pub enum Command {
    /// Broadcast a message through mixnet.
    ///
    /// A message will be split into multiple Sphinx packets, mixed through mixnet,
    /// reconstructed to the original message, and broadcasted to the entire network.
    Broadcast {
        topic: Topic,
        message: Box<[u8]>,
    },
    Subscribe(Topic),
    Unsubscribe(Topic),
    Info {
        reply: oneshot::Sender<Libp2pInfo>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixnetMessage {
    pub topic: Topic,
    pub message: Box<[u8]>,
}

impl MixnetMessage {
    pub fn as_bytes(&self) -> Box<[u8]> {
        wire::serialize(self)
            .expect("Couldn't serialize MixnetMessage")
            .into_boxed_slice()
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, wire::Error> {
        wire::deserialize(data)
    }
}
