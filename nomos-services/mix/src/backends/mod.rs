#[cfg(feature = "libp2p")]
pub mod libp2p;

use std::{fmt::Debug, hash::Hash, pin::Pin};

use futures::Stream;
use nomos_mix::membership::Membership;
use nomos_mix_message::sphinx::SphinxMessage;
use overwatch_rs::overwatch::handle::OverwatchHandle;
use rand::RngCore;

/// A trait for mix backends that send messages to the mix network.
#[async_trait::async_trait]
pub trait MixBackend {
    type Settings: Clone + Debug + Send + Sync + 'static;
    type Address: Eq + Hash + Clone;

    fn new<R>(
        config: Self::Settings,
        overwatch_handle: OverwatchHandle,
        membership: Membership<Self::Address, SphinxMessage>,
        rng: R,
    ) -> Self
    where
        R: RngCore + Send + 'static;
    /// Publish a message to the mix network.
    async fn publish(&self, msg: Vec<u8>);
    /// Listen to messages received from the mix network.
    fn listen_to_incoming_messages(&mut self) -> Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>;
}
