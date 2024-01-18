pub mod adapters;
mod messages;

// std

// crates
use futures::{Future, Stream};
// internal
use nomos_network::backends::NetworkBackend;
use nomos_network::NetworkService;
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;

pub trait NetworkAdapter {
    type Backend: NetworkBackend + 'static;
    type Settings: Clone;

    type Item: Send + Sync + 'static;
    type Key: Send + Sync + 'static;
    fn new(
        settings: Self::Settings,
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> impl Future<Output = Self> + Send
    where
        Self: Sized;

    fn transactions_stream(
        &self,
    ) -> impl Future<Output = impl Stream<Item = (Self::Key, Self::Item)> + Unpin + Send> + Send;

    fn send(&self, item: Self::Item) -> impl Future<Output = ()> + Send;
}
