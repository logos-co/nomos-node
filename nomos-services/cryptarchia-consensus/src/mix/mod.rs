pub mod adapters;

use std::pin::Pin;

// crates
use futures::Stream;
use nomos_mix_service::{backends::NetworkBackend, NetworkService};
// internal
use overwatch_rs::services::ServiceData;
use overwatch_rs::{services::relay::OutboundRelay, DynError};

#[async_trait::async_trait]
pub trait NetworkAdapter {
    type Backend: NetworkBackend + 'static;
    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;
    async fn mix(&self, message: Vec<u8>);
    async fn mixed_messages_stream(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>, DynError>;
}
