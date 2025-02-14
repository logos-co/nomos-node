// std
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
// crates
use libp2p::PeerId;
// internal
use crate::protocols::dispersal::executor::behaviour::DispersalError as ExecutorDispersalError;
use crate::protocols::dispersal::validator::behaviour::DispersalError as ValidatorDispersalError;
use crate::protocols::replication::behaviour::ReplicationError;
use crate::protocols::sampling::behaviour::SamplingError;

#[derive(Default)]
pub struct PeerStats {
    dispersal_failures: AtomicUsize,
    sampling_failures: AtomicUsize,
    replication_failures: AtomicUsize,
}

pub struct ConnectionMonitor {
    peer_stats: HashMap<PeerId, PeerStats>,
}

impl ConnectionMonitor {
    /// Initializes the connection monitor.
    pub fn new() -> Self {
        Self {
            peer_stats: HashMap::new(),
        }
    }

    fn record_failure<F>(&mut self, peer_id: Option<&PeerId>, update_counter: F)
    where
        F: Fn(&PeerStats),
    {
        if let Some(peer_id) = peer_id {
            let stats = self.peer_stats.entry(*peer_id).or_default();
            update_counter(stats);
        }
    }
    pub fn record_executor_dispersal_error(&mut self, error: &ExecutorDispersalError) {
        self.record_failure(error.peer_id(), |stats| {
            stats.dispersal_failures.fetch_add(1, Ordering::Relaxed);
        });
    }

    pub fn record_validator_dispersal_error(&mut self, error: &ValidatorDispersalError) {
        self.record_failure(error.peer_id(), |stats| {
            stats.dispersal_failures.fetch_add(1, Ordering::Relaxed);
        });
    }

    pub fn record_sampling_error(&mut self, error: &SamplingError) {
        self.record_failure(error.peer_id(), |stats| {
            stats.sampling_failures.fetch_add(1, Ordering::Relaxed);
        });
    }

    pub fn record_replication_error(&mut self, error: &ReplicationError) {
        self.record_failure(error.peer_id(), |stats| {
            stats.replication_failures.fetch_add(1, Ordering::Relaxed);
        });
    }

    pub fn get_peer_stats(&self, peer_id: &PeerId) -> Option<&PeerStats> {
        self.peer_stats.get(peer_id)
    }

    pub fn reset_peer_stats(&mut self, peer_id: &PeerId) {
        self.peer_stats.remove(peer_id);
    }
}
