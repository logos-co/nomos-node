pub mod adapters;

use std::hash::Hash;

use nomos_blend_service::{backends::BlendBackend, network::NetworkAdapter, BlendService};
use nomos_core::block::Block;
use overwatch_rs::services::{relay::OutboundRelay, ServiceData};
use serde::{de::DeserializeOwned, Serialize};

#[async_trait::async_trait]
pub trait BlendAdapter {
    type Settings: Clone + 'static;
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
