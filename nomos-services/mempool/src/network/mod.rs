pub mod adapters;
mod messages;

use futures::Stream;
use nomos_network::{backends::NetworkBackend, NetworkService};
use overwatch::services::{relay::OutboundRelay, ServiceData};

#[async_trait::async_trait]
pub trait NetworkAdapter<RuntimeServiceId> {
    type Backend: NetworkBackend + 'static;
    type Settings: Clone;
    type Payload: Send + Sync + 'static;
    type Key: Send + Sync + 'static;

    async fn new(
        settings: Self::Settings,
        network_relay: OutboundRelay<
            <NetworkService<Self::Backend, RuntimeServiceId> as ServiceData>::Message,
        >,
    ) -> Self;

    async fn payload_stream(
        &self,
    ) -> Box<dyn Stream<Item = (Self::Key, Self::Payload)> + Unpin + Send>;

    async fn send(&self, payload: Self::Payload);
}
