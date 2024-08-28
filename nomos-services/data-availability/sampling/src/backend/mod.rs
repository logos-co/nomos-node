pub mod kzgrs;

// std
use std::collections::BTreeSet;

// crates
//
// internal
use nomos_da_network_core::SubnetworkId;

#[async_trait::async_trait]
pub trait DaSamplingServiceBackend {
    type Settings;
    type BlobId;
    type Blob;

    fn new(settings: Self::Settings) -> Self;
    async fn get_validated_blobs(&self) -> BTreeSet<Self::BlobId>;
    async fn mark_in_block(&mut self, blobs_ids: &[Self::BlobId]);
    async fn handle_sampling_success(&mut self, blob_id: Self::BlobId, blob: Self::Blob);
    async fn handle_sampling_error(&mut self, blob_id: Self::BlobId);
    async fn init_sampling(&mut self, blob_id: Self::BlobId) -> Vec<SubnetworkId>;
}
