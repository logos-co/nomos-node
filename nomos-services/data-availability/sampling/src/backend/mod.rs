pub mod sampler;

// std
use std::collections::BTreeSet;
use std::fmt::Debug;
// crates
//
// internal
use crate::network::NetworkAdapter;
use nomos_da_network_service::backends::NetworkBackend;
use overwatch_rs::services::state::ServiceState;

pub trait DaSamplingServiceBackend<Backend, S>
where
    Backend: NetworkBackend + 'static,
    Backend::Settings: Clone + Debug + Send + Sync + 'static,
    Backend::State: ServiceState<Settings = Backend::Settings> + Clone + Send + Sync,
    Backend::Message: Debug + Send + Sync + 'static,
    Backend::EventKind: Debug + Send + Sync + 'static,
    Backend::NetworkEvent: Debug + Send + Sync + 'static,
    S: Clone,
{
    type Settings;
    type BlobId;
    type Blob;

    fn new(
        settings: Self::Settings,
        network_adapter: NetworkAdapter<Backend = Backend, Settings = Clone>,
    ) -> Self;
    fn trigger_sampling(&self, blob_id: Self::BlobId);
    fn get_validated_blobs(&self) -> BTreeSet<Self::BlobId>;
    fn mark_blob_validated(&self, blob_id: Self::BlobId);
}
