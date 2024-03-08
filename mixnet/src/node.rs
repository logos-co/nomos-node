use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{error::MixnetError, packet::PacketBody};

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
}
