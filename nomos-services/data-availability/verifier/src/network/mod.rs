pub mod adapters;

use futures::Stream;
use nomos_network::backends::NetworkBackend;
use nomos_network::NetworkService;
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
use overwatch_rs::DynError;

#[async_trait::async_trait]
pub trait NetworkAdapter {
    type Backend: NetworkBackend + 'static;
    type Settings;

    type Blob;
    type Attestation;

    async fn new(
        settings: Self::Settings,
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;

    async fn blob_stream(&self) -> Box<dyn Stream<Item = Self::Blob> + Unpin + Send>;
    async fn send_attestation(&self, attestation: Self::Attestation) -> Result<(), DynError>;
}
