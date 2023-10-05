pub mod adapters;
mod messages;

// std

// crates
use futures::Stream;
// internal
use nomos_network::backends::NetworkBackend;
use nomos_network::NetworkService;
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;

#[async_trait::async_trait]
pub trait NetworkAdapter {
    type Backend: NetworkBackend + 'static;
    type Settings: Clone;

    type Item: Send + Sync + 'static;
    type Key: Send + Sync + 'static;
    async fn new(
        settings: Self::Settings,
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;
    async fn transactions_stream(
        &self,
    ) -> Box<dyn Stream<Item = (Self::Key, Self::Item)> + Unpin + Send>;

    async fn send(&self, item: Self::Item);
}
