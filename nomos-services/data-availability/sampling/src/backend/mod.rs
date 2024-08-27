pub mod kzgrs;

// std
use async_trait::async_trait;
use std::collections::BTreeSet;
// crates
//
// internal
use crate::network::NetworkAdapter;
use nomos_core::da::BlobId;
use nomos_da_network_service::backends::NetworkBackend;
use overwatch_rs::services::state::ServiceState;

#[async_trait::async_trait]
pub trait DaSamplingServiceBackend {
    type Settings;
    type BlobId;
    type Blob;

    fn new(settings: Self::Settings) -> Self;
    async fn get_validated_blobs(&self) -> BTreeSet<Self::BlobId>;
    async fn mark_in_block(&mut self, blobs_id: &[Self::BlobId]);
    async fn handle_sampling_success(&mut self, blob_id: Self::BlobId, blob: Self::Blob);
    async fn handle_sampling_error(&mut self, blob_id: Self::BlobId);
}
