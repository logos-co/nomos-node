pub mod adapters;

// std
use nomos_core::block::Block;
use std::hash::Hash;
// crates
use nomos_mix_service::network::NetworkAdapter;
use nomos_mix_service::{backends::MixBackend, MixService};
// internal
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
use serde::de::DeserializeOwned;
use serde::Serialize;

#[async_trait::async_trait]
pub trait MixAdapter {
    type Settings: Clone + 'static;
    type Backend: MixBackend + 'static;
    type Network: NetworkAdapter + 'static;
    type Tx: Serialize + DeserializeOwned + Clone + Eq + Hash + 'static;
    type BlobCertificate: Serialize + DeserializeOwned + Clone + Eq + Hash + 'static;
    async fn new(
        settings: Self::Settings,
        mix_relay: OutboundRelay<
            <MixService<Self::Backend, Self::Network> as ServiceData>::Message,
        >,
    ) -> Self;
    async fn mix(&self, block: Block<Self::Tx, Self::BlobCertificate>);
}
