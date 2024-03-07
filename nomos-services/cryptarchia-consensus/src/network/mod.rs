pub mod adapters;
pub mod messages;

// std
// crates
use futures::Stream;
// internal
use crate::network::messages::{BlockMsg, NetworkMessage};
use nomos_network::backends::NetworkBackend;
use nomos_network::NetworkService;
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;

type BoxedStream<T> = Box<dyn Stream<Item = T> + Send + Sync + Unpin>;

#[async_trait::async_trait]
pub trait NetworkAdapter {
    type Backend: NetworkBackend + 'static;
    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;
    async fn blocks_stream(&self) -> Box<dyn Stream<Item = BlockMsg> + Send + Sync + Unpin>;
    async fn broadcast(&self, message: NetworkMessage);
}
