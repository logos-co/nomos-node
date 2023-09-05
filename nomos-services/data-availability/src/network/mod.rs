// std
// crates
use futures::Stream;
use tokio::sync::oneshot;
// internal
use nomos_network::backends::NetworkBackend;
use nomos_network::NetworkService;
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;

#[async_trait::async_trait]
pub trait NetworkAdapter {
    type Backend: NetworkBackend + 'static;

    type Blob: Send + Sync + 'static;
    type Reply: Send + Sync + 'static;

    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;

    async fn blob_stream(
        &self,
    ) -> Box<dyn Stream<Item = (Self::Blob, oneshot::Sender<Self::Reply>)>>;
}
