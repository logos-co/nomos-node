pub mod kzgrs;

// std
use std::collections::BTreeSet;
// crates
use rand::Rng;
use tokio::time::Interval;
// internal
use nomos_da_network_core::SubnetworkId;

pub enum SamplingState {
    Init(Vec<SubnetworkId>),
    Tracking,
    Terminated,
}

#[async_trait::async_trait]
pub trait DaSamplingServiceBackend<R: Rng> {
    type Settings;
    type BlobId;
    type Blob;

    fn new(settings: Self::Settings, rng: R) -> Self;
    async fn get_validated_blobs(&self) -> BTreeSet<Self::BlobId>;
    async fn mark_in_block(&mut self, blobs_ids: &[Self::BlobId]);
    async fn handle_sampling_success(&mut self, blob_id: Self::BlobId, blob: Self::Blob);
    async fn handle_sampling_error(&mut self, blob_id: Self::BlobId);
    async fn init_sampling(&mut self, blob_id: Self::BlobId) -> SamplingState;
    fn prune_interval(&self) -> Interval;
    fn prune(&mut self);
}
