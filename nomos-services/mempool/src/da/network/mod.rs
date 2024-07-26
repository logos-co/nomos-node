use std::marker::PhantomData;

use kzgrs_backend::common::blob::DaBlob;
// crates
use nomos_da_network_core::SubnetworkId;

// internal
pub mod adapters;
// std

// TODO: remove after using it in the service
#[allow(dead_code)]
#[async_trait::async_trait]
pub trait DaNetworkAdapter {
    // TODO: bound to proper da-backend trait
    type Backend;
    type Settings: Clone;
    type Payload: Send + Sync + 'static;
    type Key: Send + Sync + 'static;

    async fn new(
        settings: Self::Settings,
        // TODO: hook da-network service instead of network service
        // network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
        network_relay: PhantomData<()>,
    ) -> Self;

    async fn sample(&self, blob_id: &[u8], subnetwork_id: SubnetworkId) -> Option<DaBlob>;
}
