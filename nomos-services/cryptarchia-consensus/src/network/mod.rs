pub mod adapters;
pub mod messages;

// std
use std::hash::Hash;
// crates
use futures::Stream;
use nomos_core::block::Block;
// internal
use crate::network::messages::NetworkMessage;
use nomos_network::backends::NetworkBackend;
use nomos_network::NetworkService;
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
use serde::{de::DeserializeOwned, Serialize};

type BoxedStream<T> = Box<dyn Stream<Item = T> + Send + Sync + Unpin>;

#[async_trait::async_trait]
pub trait NetworkAdapter {
    type Backend: NetworkBackend + 'static;
    type Tx: Serialize + DeserializeOwned + Clone + Eq + Hash + 'static;
    type BlobCertificate: Serialize + DeserializeOwned + Clone + Eq + Hash + 'static;
    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;
    async fn blocks_stream(&self) -> BoxedStream<Block<Self::Tx, Self::BlobCertificate>>;
    async fn broadcast(&self, message: NetworkMessage<Self::Tx, Self::BlobCertificate>);
}
