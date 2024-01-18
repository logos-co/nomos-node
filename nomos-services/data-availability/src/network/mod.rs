pub mod adapters;

// std
// crates
use futures::{Future, Stream};
// internal
use nomos_network::backends::NetworkBackend;
use nomos_network::NetworkService;
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
use overwatch_rs::DynError;
use serde::de::DeserializeOwned;
use serde::Serialize;

pub trait NetworkAdapter {
    type Backend: NetworkBackend + 'static;

    type Blob: Serialize + DeserializeOwned + Send + Sync + 'static;
    type Attestation: Serialize + DeserializeOwned + Send + Sync + 'static;

    fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> impl Future<Output = Self> + Send
    where
        Self: Sized;

    fn blob_stream(
        &self,
    ) -> impl Future<Output = impl Stream<Item = Self::Blob> + Unpin + Send> + Send;

    fn attestation_stream(
        &self,
    ) -> impl Future<Output = impl Stream<Item = Self::Attestation> + Unpin + Send> + Send;

    fn send_attestation(
        &self,
        attestation: Self::Attestation,
    ) -> impl Future<Output = Result<(), DynError>> + Send;

    fn send_blob(&self, blob: Self::Blob) -> impl Future<Output = Result<(), DynError>> + Send;
}
