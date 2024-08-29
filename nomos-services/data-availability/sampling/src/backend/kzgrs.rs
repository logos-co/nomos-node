// std
use std::collections::{BTreeSet, HashMap};
use std::fmt::Debug;
use std::time::{Duration, Instant};

// crates
use rand::distributions::Standard;
use rand::prelude::*;
use rand::SeedableRng;

use kzgrs_backend::common::blob::DaBlob;

//
// internal
use super::DaSamplingServiceBackend;
use nomos_core::da::BlobId;
use nomos_da_network_core::SubnetworkId;

pub struct SamplingContext {
    subnets: Vec<SubnetworkId>,
    started: Instant,
}

#[derive(Debug, Clone)]
pub struct KzgrsDaSamplerSettings {
    pub num_samples: u16,
    pub old_blobs_check_duration: Duration,
    pub blobs_validity_duration: Duration,
}

pub struct KzgrsDaSampler<R: Rng> {
    settings: KzgrsDaSamplerSettings,
    validated_blobs: BTreeSet<BlobId>,
    pending_sampling_blobs: HashMap<BlobId, SamplingContext>,
    // TODO: is there a better place for this? Do we need to have this even globally?
    // Do we already have some source of randomness already?
    rng: R,
}

impl<R: Rng> KzgrsDaSampler<R> {
    fn prune_by_time(&mut self) {
        self.pending_sampling_blobs.retain(|_blob_id, context| {
            context.started.elapsed() < self.settings.old_blobs_check_duration
        });
    }
}

#[async_trait::async_trait]
impl<R: Rng + Sync + Send> DaSamplingServiceBackend<R> for KzgrsDaSampler<R> {
    type Settings = KzgrsDaSamplerSettings;
    type BlobId = BlobId;
    type Blob = DaBlob;

    fn new(settings: Self::Settings, rng: R) -> Self {
        let bt: BTreeSet<BlobId> = BTreeSet::new();
        Self {
            settings,
            validated_blobs: bt,
            pending_sampling_blobs: HashMap::new(),
            rng: rng,
        }
    }

    async fn get_validated_blobs(&self) -> BTreeSet<Self::BlobId> {
        self.validated_blobs.clone()
    }

    async fn mark_in_block(&mut self, blobs_ids: &[Self::BlobId]) {
        for id in blobs_ids {
            self.pending_sampling_blobs.remove(id);
            self.validated_blobs.remove(id);
        }
    }

    async fn handle_sampling_success(&mut self, blob_id: Self::BlobId, blob: Self::Blob) {
        if let Some(ctx) = self.pending_sampling_blobs.get_mut(&blob_id) {
            ctx.subnets.push(blob.column_idx as SubnetworkId);

            // sampling of this blob_id terminated successfully
            if ctx.subnets.len() == self.settings.num_samples as usize {
                self.validated_blobs.insert(blob_id);
                // cleanup from pending samplings
                self.pending_sampling_blobs.remove(&blob_id);
            }
        } else {
            unreachable!("We should not receive a sampling success from a non triggered blobId");
        }
    }

    async fn handle_sampling_error(&mut self, blob_id: Self::BlobId) {
        // If it fails a single time we consider it failed.
        // We may want to abstract the sampling policies somewhere else at some point if we
        // need to get fancier than this
        self.pending_sampling_blobs.remove(&blob_id);
        self.validated_blobs.remove(&blob_id);
    }

    async fn init_sampling(&mut self, blob_id: Self::BlobId) -> Vec<SubnetworkId> {
        let subnets: Vec<SubnetworkId> = Standard
            .sample_iter(&mut self.rng)
            .take(self.settings.num_samples as usize)
            .collect();
        let ctx: SamplingContext = SamplingContext {
            subnets: subnets.clone(),
            started: Instant::now(),
        };
        self.pending_sampling_blobs.insert(blob_id, ctx);
        subnets
    }

    fn prune(&mut self) {
        self.prune_by_time()
    }
}
