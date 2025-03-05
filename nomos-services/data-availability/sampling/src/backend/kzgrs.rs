use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fmt::Debug,
    sync::Arc,
    time::{Duration, Instant},
};

use hex;
use kzgrs_backend::common::{
    blob::{DaBlob, DaBlobSharedCommitments},
    ColumnIndex,
};
use nomos_core::da::BlobId;
use nomos_da_network_core::SubnetworkId;
use nomos_tracing::info_with_id;
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::{time, time::Interval};
use tracing::instrument;

use crate::{backend::SamplingState, DaSamplingServiceBackend};

#[derive(Clone)]
pub struct SamplingContext {
    subnets: HashSet<SubnetworkId>,
    started: Instant,
    commitment: Option<Arc<DaBlobSharedCommitments>>,
}

impl SamplingContext {
    fn new(commitment: Option<DaBlobSharedCommitments>) -> Self {
        Self {
            subnets: HashSet::new(),
            started: Instant::now(),
            commitment: commitment.map(Arc::new),
        }
    }

    const fn is_waiting_commitments(&self) -> bool {
        self.commitment.is_none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KzgrsSamplingBackendSettings {
    pub num_samples: u16,
    pub num_subnets: u16,
    pub old_blobs_check_interval: Duration,
    pub blobs_validity_duration: Duration,
}

pub struct KzgrsSamplingBackend<R: Rng> {
    settings: KzgrsSamplingBackendSettings,
    validated_blobs: BTreeSet<BlobId>,
    pending_sampling_blobs: HashMap<BlobId, SamplingContext>,
    rng: R,
}

impl<R: Rng> KzgrsSamplingBackend<R> {
    fn prune_by_time(&mut self) {
        self.pending_sampling_blobs.retain(|_blob_id, context| {
            context.started.elapsed() < self.settings.blobs_validity_duration
        });
    }
}

#[async_trait::async_trait]
impl<R: Rng + Sync + Send> DaSamplingServiceBackend<R> for KzgrsSamplingBackend<R> {
    type Settings = KzgrsSamplingBackendSettings;
    type BlobId = BlobId;
    type Blob = DaBlob;
    type SharedCommitments = DaBlobSharedCommitments;

    fn new(settings: Self::Settings, rng: R) -> Self {
        let bt: BTreeSet<BlobId> = BTreeSet::new();
        Self {
            settings,
            validated_blobs: bt,
            pending_sampling_blobs: HashMap::new(),
            rng,
        }
    }

    fn prune_interval(&self) -> Interval {
        time::interval(self.settings.old_blobs_check_interval)
    }

    async fn get_validated_blobs(&self) -> BTreeSet<Self::BlobId> {
        self.validated_blobs.clone()
    }

    #[instrument(skip_all)]
    async fn mark_completed(&mut self, blobs_ids: &[Self::BlobId]) {
        for id in blobs_ids {
            info_with_id!(id, "MarkInBlock");
            self.pending_sampling_blobs.remove(id);
            self.validated_blobs.remove(id);
        }
    }

    async fn handle_sampling_success(&mut self, blob_id: Self::BlobId, column_idx: ColumnIndex) {
        if let Some(ctx) = self.pending_sampling_blobs.get_mut(&blob_id) {
            tracing::info!(
                "subnet {} for blob id {} has been successfully sampled",
                column_idx,
                hex::encode(blob_id)
            );
            ctx.subnets.insert(column_idx as SubnetworkId);

            // sampling of this blob_id terminated successfully
            if ctx.subnets.len() == self.settings.num_samples as usize {
                self.validated_blobs.insert(blob_id);
                tracing::info!(
                    "blob_id {} has been successfully sampled",
                    hex::encode(blob_id)
                );
                // cleanup from pending samplings
                self.pending_sampling_blobs.remove(&blob_id);
            }
        }
    }

    async fn handle_sampling_error(&mut self, blob_id: Self::BlobId) {
        // If it fails a single time we consider it failed.
        // We may want to abstract the sampling policies somewhere else at some point if
        // we need to get fancier than this
        self.pending_sampling_blobs.remove(&blob_id);
        self.validated_blobs.remove(&blob_id);
    }

    async fn prepare_sampling(&mut self, blob_id: Self::BlobId) -> SamplingState {
        if self.validated_blobs.contains(&blob_id) {
            return SamplingState::Terminated;
        }
        if self.pending_sampling_blobs.contains_key(&blob_id) {
            return SamplingState::Tracking;
        }
        self.pending_sampling_blobs
            .insert(blob_id, SamplingContext::new(None));
        SamplingState::WaitingCommitments
    }
    async fn init_sampling(
        &mut self,
        blob_id: Self::BlobId,
        commitments: Self::SharedCommitments,
    ) -> SamplingState {
        if self.validated_blobs.contains(&blob_id) {
            return SamplingState::Terminated;
        }

        match self.pending_sampling_blobs.get_mut(&blob_id) {
            Some(ctx) => {
                if ctx.is_waiting_commitments() {
                    ctx.commitment = Some(Arc::new(commitments));
                    let subnets = (0..self.settings.num_subnets as SubnetworkId)
                        .choose_multiple(&mut self.rng, self.settings.num_samples.into());

                    SamplingState::Init(subnets)
                } else {
                    SamplingState::Tracking
                }
            }
            None => SamplingState::WaitingCommitments,
        }
    }
    fn prune(&mut self) {
        self.prune_by_time();
    }

    fn commitments(&self, blob_id: &Self::BlobId) -> Option<Arc<Self::SharedCommitments>> {
        self.pending_sampling_blobs
            .get(blob_id)
            .and_then(|ctx| ctx.commitment.clone())
    }
}

#[cfg(test)]
mod test {

    use std::{
        collections::HashSet,
        time::{Duration, Instant},
    };

    use kzgrs::{Commitment, Proof};
    use kzgrs_backend::common::{
        blob::{DaBlob, DaBlobSharedCommitments},
        Column,
    };
    use nomos_core::da::BlobId;
    use rand::{prelude::*, rngs::StdRng};

    use crate::backend::kzgrs::{
        DaSamplingServiceBackend, KzgrsSamplingBackend, KzgrsSamplingBackendSettings,
        SamplingContext, SamplingState,
    };

    fn create_sampler(num_samples: usize, num_subnets: usize) -> KzgrsSamplingBackend<StdRng> {
        let settings = KzgrsSamplingBackendSettings {
            num_samples: num_samples as u16,
            num_subnets: num_subnets as u16,
            old_blobs_check_interval: Duration::from_millis(20),
            blobs_validity_duration: Duration::from_millis(10),
        };
        let rng = StdRng::from_entropy();
        KzgrsSamplingBackend::new(settings, rng)
    }

    #[tokio::test]
    async fn test_init_sampling_subnet_range() {
        let number_of_subnets = 42;
        let num_samples = 50; // Testing with more samples than subnets to check the limit
        let mut backend = create_sampler(num_samples, number_of_subnets);

        let blob_id = BlobId::default();
        let state = backend
            .init_sampling(blob_id, DaBlobSharedCommitments::default())
            .await;

        if let SamplingState::Init(subnets) = state {
            let unique_subnet_ids: HashSet<_> = subnets.iter().copied().collect();

            assert_eq!(
                unique_subnet_ids.len(),
                subnets.len(),
                "Subnet IDs are not unique"
            );

            assert_eq!(
                subnets.len(),
                number_of_subnets.min(num_samples),
                "Incorrect number of subnet IDs selected"
            );

            for subnet_id in subnets {
                assert!(
                    (subnet_id as usize) < number_of_subnets,
                    "Subnet ID is out of range"
                );
            }
        }
    }

    #[tokio::test]
    #[expect(clippy::too_many_lines, reason = "TODO: Test function")]
    async fn test_sampler() {
        // fictitious number of subnets
        let subnet_num: usize = 42;

        // create a sampler instance
        let sampler = &mut create_sampler(subnet_num, 42);

        // create some blobs and blob_ids
        let b1: BlobId = sampler.rng.gen();
        let b2: BlobId = sampler.rng.gen();
        let blob = DaBlob {
            column_idx: 42,
            column: Column(vec![]),
            column_commitment: Commitment::default(),
            aggregated_column_commitment: Commitment::default(),
            aggregated_column_proof: Proof::default(),
            rows_commitments: vec![],
            rows_proofs: vec![],
        };
        let blob2 = blob.clone();
        let mut blob3 = blob2.clone();

        // at start everything should be empty
        assert!(sampler.pending_sampling_blobs.is_empty());
        assert!(sampler.validated_blobs.is_empty());
        assert!(sampler.get_validated_blobs().await.is_empty());

        // start sampling for b1
        let SamplingState::WaitingCommitments = sampler.prepare_sampling(b1).await else {
            panic!("unexpected return value")
        };

        assert!(sampler.pending_sampling_blobs.len() == 1);
        let ctx_b1 = sampler.pending_sampling_blobs.get_mut(&b1).unwrap();
        assert!(ctx_b1.commitment.is_none());

        let SamplingState::Init(subnets_to_sample) = sampler
            .init_sampling(b1, DaBlobSharedCommitments::default())
            .await
        else {
            panic!("unexpected return value")
        };
        assert!(subnets_to_sample.len() == subnet_num);
        assert!(sampler.validated_blobs.is_empty());
        let ctx_b1 = sampler.pending_sampling_blobs.get_mut(&b1).unwrap();
        assert!(ctx_b1.commitment.is_some());

        // start sampling for b2
        let SamplingState::WaitingCommitments = sampler.prepare_sampling(b2).await else {
            panic!("unexpected return value")
        };

        assert!(sampler.pending_sampling_blobs.len() == 2);
        let ctx_b2 = sampler.pending_sampling_blobs.get_mut(&b2).unwrap();
        assert!(ctx_b2.commitment.is_none());

        let SamplingState::Init(subnets_to_sample2) = sampler
            .init_sampling(b2, DaBlobSharedCommitments::default())
            .await
        else {
            panic!("unexpected return value")
        };
        assert!(subnets_to_sample2.len() == subnet_num);
        assert!(sampler.validated_blobs.is_empty());
        let ctx_b2 = sampler.pending_sampling_blobs.get_mut(&b1).unwrap();
        assert!(ctx_b2.commitment.is_some());

        // mark in block for both
        // collections should be reset
        sampler.mark_completed(&[b1, b2]).await;
        assert!(sampler.pending_sampling_blobs.is_empty());
        assert!(sampler.validated_blobs.is_empty());

        // because they're reset, we need to restart sampling for the test
        _ = sampler.prepare_sampling(b1).await;
        _ = sampler
            .init_sampling(b1, DaBlobSharedCommitments::default())
            .await;
        _ = sampler.prepare_sampling(b2).await;
        _ = sampler
            .init_sampling(b2, DaBlobSharedCommitments::default())
            .await;

        // handle ficticious error for b2
        // b2 should be gone, b1 still around
        sampler.handle_sampling_error(b2).await;
        assert!(sampler.validated_blobs.is_empty());
        assert!(sampler.pending_sampling_blobs.len() == 1);
        assert!(sampler.pending_sampling_blobs.contains_key(&b1));

        // handle ficticious sampling success for b1
        // should still just have one pending blob, no validated blobs yet,
        // and one subnet added to blob
        sampler.handle_sampling_success(b1, blob.column_idx).await;
        assert!(sampler.validated_blobs.is_empty());
        assert!(sampler.pending_sampling_blobs.len() == 1);
        println!(
            "{}",
            sampler
                .pending_sampling_blobs
                .get(&b1)
                .unwrap()
                .subnets
                .len()
        );
        assert!(
            sampler
                .pending_sampling_blobs
                .get(&b1)
                .unwrap()
                .subnets
                .len()
                == 1
        );

        // handle_success for always the same subnet
        // by adding number of subnets time the same subnet
        // (column_idx did not change)
        // should not change, still just one sampling blob,
        // no validated blobs and one subnet
        for _ in 1..subnet_num {
            let b = blob2.clone();
            sampler.handle_sampling_success(b1, b.column_idx).await;
        }
        assert!(sampler.validated_blobs.is_empty());
        assert!(sampler.pending_sampling_blobs.len() == 1);
        assert!(
            sampler
                .pending_sampling_blobs
                .get(&b1)
                .unwrap()
                .subnets
                .len()
                == 1
        );

        // handle_success for up to subnet size minus one subnet
        // should still not change anything
        // but subnets len is now subnet size minus one
        // we already added subnet 42
        for i in 1..(subnet_num - 1) {
            let mut b = blob2.clone();
            b.column_idx = i as u16;
            sampler.handle_sampling_success(b1, b.column_idx).await;
        }
        assert!(sampler.validated_blobs.is_empty());
        assert!(sampler.pending_sampling_blobs.len() == 1);
        assert!(
            sampler
                .pending_sampling_blobs
                .get(&b1)
                .unwrap()
                .subnets
                .len()
                == subnet_num - 1
        );

        // now add the last subnet!
        // we should have all subnets set,
        // and the validated blobs should now have that blob
        // pending blobs should now be empty
        blob3.column_idx = (subnet_num - 1) as u16;
        sampler.handle_sampling_success(b1, blob3.column_idx).await;
        assert!(sampler.pending_sampling_blobs.is_empty());
        assert!(sampler.validated_blobs.len() == 1);
        assert!(sampler.validated_blobs.contains(&b1));
        // these checks are redundant but better safe than sorry
        assert!(sampler.get_validated_blobs().await.len() == 1);
        assert!(sampler.get_validated_blobs().await.contains(&b1));

        // run mark_in_block for the same blob
        // should return empty for everything
        sampler.mark_completed(&[b1]).await;
        assert!(sampler.validated_blobs.is_empty());
        assert!(sampler.pending_sampling_blobs.is_empty());
    }

    #[tokio::test]
    async fn test_pruning() {
        let mut sampler = create_sampler(42, 42);

        // create some sampling contexes
        // first set will go through as in time
        let ctx1 = SamplingContext {
            subnets: HashSet::new(),
            started: Instant::now(),
            commitment: None,
        };
        let ctx2 = ctx1.clone();
        let ctx3 = ctx1.clone();

        // second set: will fail for expired
        let ctx11 = SamplingContext {
            subnets: HashSet::new(),
            started: Instant::now().checked_sub(Duration::from_secs(1)).unwrap(),
            commitment: None,
        };
        let ctx12 = ctx11.clone();
        let ctx13 = ctx11.clone();

        // create a couple blob ids
        let b1: BlobId = sampler.rng.gen();
        let b2: BlobId = sampler.rng.gen();
        let b3: BlobId = sampler.rng.gen();

        // insert first blob
        // pruning should have no effect
        assert!(sampler.pending_sampling_blobs.is_empty());
        sampler.pending_sampling_blobs.insert(b1, ctx1);
        sampler.prune();
        assert!(sampler.pending_sampling_blobs.len() == 1);

        // insert second blob
        // pruning should have no effect
        sampler.pending_sampling_blobs.insert(b2, ctx2);
        sampler.prune();
        assert!(sampler.pending_sampling_blobs.len() == 2);

        // insert third blob
        // pruning should have no effect
        sampler.pending_sampling_blobs.insert(b3, ctx3);
        sampler.prune();
        assert!(sampler.pending_sampling_blobs.len() == 3);

        // insert fake expired blobs
        // pruning these should now decrease pending blobx every time
        sampler.pending_sampling_blobs.insert(b1, ctx11);
        sampler.prune();
        assert!(sampler.pending_sampling_blobs.len() == 2);

        sampler.pending_sampling_blobs.insert(b2, ctx12);
        sampler.prune();
        assert!(sampler.pending_sampling_blobs.len() == 1);

        sampler.pending_sampling_blobs.insert(b3, ctx13);
        sampler.prune();
        assert!(sampler.pending_sampling_blobs.is_empty());
    }
}
