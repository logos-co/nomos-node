pub mod adapters;

use std::pin::Pin;

// crates
use futures::Stream;
use nomos_mix_service::{backends::MixBackend, MixService};
// internal
use overwatch_rs::services::ServiceData;
use overwatch_rs::{services::relay::OutboundRelay, DynError};

#[async_trait::async_trait]
pub trait MixAdapter {
    type Backend: MixBackend + 'static;
    async fn new(
        network_relay: OutboundRelay<<MixService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;
    async fn mix(&self, message: Vec<u8>);
    async fn mixed_messages_stream(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>, DynError>;
}
