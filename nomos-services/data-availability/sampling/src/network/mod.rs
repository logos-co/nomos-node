pub mod adapters;

use futures::Stream;
use nomos_core::da::BlobId;
use nomos_da_network_core::SubnetworkId;
use nomos_da_network_service::backends::libp2p::common::SamplingEvent;
use nomos_da_network_service::backends::NetworkBackend;
use nomos_da_network_service::NetworkService;
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
use overwatch_rs::DynError;
use std::pin::Pin;

#[async_trait::async_trait]
pub trait NetworkAdapter {
    type Backend: NetworkBackend + Send + 'static;
    type Settings: Clone;

    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;

    async fn start_sampling(
        &mut self,
        blob_id: BlobId,
        subnets: &[SubnetworkId],
    ) -> Result<(), DynError>;
    async fn listen_to_sampling_messages(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = SamplingEvent> + Send>>, DynError>;
}
