use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::error::MixnetError;

/// Mix node implementation that returns [`Output`] if exists.
pub struct MixNode {}

/// Mix node configuration
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixNodeConfig {}

/// Queue for sending packets to [`MixNode`]
pub type PacketQueue = mpsc::Sender<Box<[u8]>>;

impl MixNode {
    pub fn new(_: MixNodeConfig) -> Result<(Self, PacketQueue), MixnetError> {
        todo!()
    }
}
