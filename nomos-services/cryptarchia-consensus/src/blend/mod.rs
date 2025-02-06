pub mod adapters;

// std
use nomos_core::block::Block;
use std::hash::Hash;
// crates
use nomos_blend_service::network::NetworkAdapter;
use nomos_blend_service::{backends::BlendBackend, BlendService};
// internal
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
use serde::de::DeserializeOwned;
use serde::Serialize;

#[async_trait::async_trait]
pub trait BlendAdapter {
    type Settings: Clone + Send + 'static;
    type Backend: BlendBackend + 'static;
    type Network: NetworkAdapter + 'static;
    type Tx: Serialize + DeserializeOwned + Clone + Eq + Hash + 'static;
    type BlobCertificate: Serialize + DeserializeOwned + Clone + Eq + Hash + 'static;
    async fn new(
        settings: Self::Settings,
        blend_relay: OutboundRelay<
            <BlendService<Self::Backend, Self::Network> as ServiceData>::Message,
        >,
    ) -> Self;
    async fn blend(&self, block: Block<Self::Tx, Self::BlobCertificate>);
}
