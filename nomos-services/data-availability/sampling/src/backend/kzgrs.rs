use std::borrow::BorrowMut;
// std
use std::collections::{BTreeSet, HashMap};
use std::fmt::Debug;
use std::thread;
use std::time::Duration;

// crates
use chrono::{naive::NaiveDateTime, Utc};
use rand::distributions::Standard;
use rand::prelude::*;
use rand_chacha::ChaCha20Rng;

use kzgrs_backend::common::blob::DaBlob;

//
// internal
use super::DaSamplingServiceBackend;
use nomos_core::da::BlobId;
use nomos_da_network_core::SubnetworkId;

pub struct SamplingContext {
    blob_id: BlobId,
    subnets: Vec<SubnetworkId>,
    started: NaiveDateTime,
}

#[derive(Debug, Clone)]
pub struct KzgrsDaSamplerSettings {
    pub num_samples: u16,
    pub old_blobs_check_duration: Duration,
    pub blobs_validity_duration: Duration,
}

pub struct KzgrsDaSampler {
    settings: KzgrsDaSamplerSettings,
    validated_blobs: BTreeSet<BlobId>,
    // TODO: This needs to be properly synchronized, if this is going to be accessed
    // by independent threads (monitoring thread)
    pending_sampling_blobs: HashMap<BlobId, SamplingContext>,
    // TODO: is there a better place for this? Do we need to have this even globally?
    // Do we already have some source of randomness already?
    rng: ChaCha20Rng,
}

impl KzgrsDaSampler {
    // TODO: this might not be the right signature, as the lifetime of self needs to be evaluated
    async fn start_pending_blob_monitor(&'static mut self) {
        //let mut sself = self;
        let monitor = thread::spawn(move || {
            loop {
                thread::sleep(self.settings.old_blobs_check_duration);
                // everything older than cut_timestamp should be removed;
                let cut_timestamp = Utc::now().naive_utc() - self.settings.blobs_validity_duration;
                // retain all elements which come after the cut_timestamp
                self.pending_sampling_blobs
                    .retain(|_, ctx| ctx.started.gt(&cut_timestamp));
            }
        });
        monitor.join().unwrap();
    }
}

#[async_trait::async_trait]
impl<'a> DaSamplingServiceBackend for KzgrsDaSampler {
    type Settings = KzgrsDaSamplerSettings;
    type BlobId = BlobId;
    type Blob = DaBlob;

    fn new(settings: Self::Settings) -> Self {
        let bt: BTreeSet<BlobId> = BTreeSet::new();
        Self {
            settings: settings,
            validated_blobs: bt,
            pending_sampling_blobs: HashMap::new(),
            rng: ChaCha20Rng::from_entropy(),
        }
        // TODO: how to start the actual monitoring thread with the correct ownership/lifetime?
    }

    async fn get_validated_blobs(&self) -> BTreeSet<Self::BlobId> {
        self.validated_blobs.clone()
    }

    async fn mark_in_block(&mut self, blobs_ids: &[Self::BlobId]) {
        for id in blobs_ids {
            if self.pending_sampling_blobs.contains_key(id) {
                self.pending_sampling_blobs.remove(id);
            }

            if self.validated_blobs.contains(id) {
                self.validated_blobs.remove(id);
            }
        }
    }

    async fn handle_sampling_success(&mut self, blob_id: Self::BlobId, blob: Self::Blob) {
        // this should not even happen
        if !self.pending_sampling_blobs.contains_key(&blob_id) {}

        let ctx = self.pending_sampling_blobs.get_mut(&blob_id).unwrap();
        ctx.subnets.push(blob.column_idx as SubnetworkId);

        // sampling of this blob_id terminated successfully
        if ctx.subnets.len() == self.settings.num_samples as usize {
            self.validated_blobs.insert(blob_id);
        }
    }

    async fn handle_sampling_error(&mut self, _blob_id: Self::BlobId) {
        // TODO: Unimplmented yet because the error handling in the service
        // does not yet receive a blob_id
        unimplemented!("no use case yet")
    }

    async fn init_sampling(&mut self, blob_id: Self::BlobId) -> Vec<SubnetworkId> {
        let mut ctx: SamplingContext = SamplingContext {
            blob_id: (blob_id),
            subnets: vec![],
            started: Utc::now().naive_utc(),
        };

        let subnets: Vec<SubnetworkId> = Standard
            .sample_iter(&mut self.rng)
            .take(self.settings.num_samples as usize)
            .collect();
        ctx.subnets = subnets.clone();

        subnets
    }
}
