use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{error::MixnetError, packet::Packet, packet::PacketBody};

/// Mix node implementation that returns Sphinx packets which needs to be forwarded to next mix nodes,
/// or messages reconstructed from Sphinx packets delivered through all mix layers.
pub struct MixNode {}

/// Mix node configuration
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixNodeConfig {}

/// Queue for sending packets to [`MixNode`]
pub type PacketQueue = mpsc::Sender<PacketBody>;

impl MixNode {
    pub fn new(_: MixNodeConfig) -> Result<(Self, PacketQueue), MixnetError> {
        todo!()
    }

    /// Returns a next `[Output]` to be emitted, if it exists and the Poisson delay is done (if necessary).
    pub async fn next(&mut self) -> Option<Output> {
        todo!()
    }
}

/// Output that [`MixNode::next`] returns.
#[derive(Debug, PartialEq, Eq)]
pub enum Output {
    /// Packet to be forwarded to the next mix node
    Forward(Packet),
    /// Message reconstructed from [`Packet`]s
    ReconstructedMessage(Box<[u8]>),
}
