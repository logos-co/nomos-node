pub mod adapters;

// crates
use futures::Stream;
use nomos_mix_service::{backends::NetworkBackend, NetworkService};
// internal
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;

type BoxedStream<T> = Box<dyn Stream<Item = T> + Send + Sync + Unpin>;

#[async_trait::async_trait]
pub trait NetworkAdapter {
    type Backend: NetworkBackend + 'static;
    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;
    async fn mix(&self, message: Vec<u8>);
    async fn mixed_msgs_stream(&self) -> BoxedStream<Vec<u8>>;
}
