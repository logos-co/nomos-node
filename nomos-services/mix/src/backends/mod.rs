#[cfg(feature = "libp2p")]
pub mod libp2p;

use std::{fmt::Debug, pin::Pin};

use futures::Stream;
use overwatch_rs::overwatch::handle::OverwatchHandle;

/// A trait for mix backends that send messages to the mix network.
#[async_trait::async_trait]
pub trait MixBackend {
    type Settings: Clone + Debug + Send + Sync + 'static;

    fn new(config: Self::Settings, overwatch_handle: OverwatchHandle) -> Self;
    async fn mix(&self, msg: Vec<u8>);
    /// Listen to fully unwrapped messages returned from the mix network
    /// if this node is the last node that can unwrap the data message.
    fn listen_to_fully_unwrapped_messages(&mut self)
        -> Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>;
}
