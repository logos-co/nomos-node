use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::error::MixnetError;

/// Mix client implementation that is used to schedule messages to be sent to the mixnet.
/// Messages inserted to the [`MessageQueue`] are scheduled according to the Poisson interals
/// and returns from [`MixClient.next()`] when it is ready to be sent to the mixnet.
/// If there is no messages inserted to the [`MessageQueue`], cover packets are generated and
/// returned from [`MixClient.next()`].
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
