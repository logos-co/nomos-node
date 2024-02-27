use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::error::MixnetError;

/// Mix client implementation that returns packets in Poisson intervals
pub struct MixClient {}

/// Mix client configuration
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixClientConfig {}

/// Queue for sending messages to [`MixClient`]
pub type MessageQueue = mpsc::Sender<Box<[u8]>>;

impl MixClient {
    pub fn new(_: MixClientConfig) -> Result<(Self, MessageQueue), MixnetError> {
        todo!()
    }
}
