pub mod kzgrs;

use std::{collections::BTreeSet, sync::Arc};

use kzgrs_backend::common::ColumnIndex;
use nomos_da_network_core::SubnetworkId;
use rand::Rng;
use tokio::time::Interval;

pub enum SamplingState {
    WaitingCommitments,
    Init(Vec<SubnetworkId>),
    Tracking,
    Terminated,
}

#[async_trait::async_trait]
pub trait DaSamplingServiceBackend<R: Rng> {
    type Settings;
    type BlobId;
    type Blob;
    type SharedCommitments;
    fn new(settings: Self::Settings, rng: R) -> Self;
    async fn get_validated_blobs(&self) -> BTreeSet<Self::BlobId>;
    async fn mark_completed(&mut self, blobs_ids: &[Self::BlobId]);
    async fn handle_sampling_success(&mut self, blob_id: Self::BlobId, column_index: ColumnIndex);
    async fn handle_sampling_error(&mut self, blob_id: Self::BlobId);
    async fn prepare_sampling(&mut self, blob_id: Self::BlobId) -> SamplingState;
    async fn init_sampling(
        &mut self,
        blob_id: Self::BlobId,
        commitments: Self::SharedCommitments,
    ) -> SamplingState;
    fn prune_interval(&self) -> Interval;
    fn prune(&mut self);

    fn commitments(&self, blob_id: &Self::BlobId) -> Option<Arc<Self::SharedCommitments>>;
}
