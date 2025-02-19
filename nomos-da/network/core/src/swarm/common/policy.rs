// std
// crates
use serde::{Deserialize, Serialize};
// internal
use crate::swarm::common::monitor::{PeerHealthPolicy, PeerStats};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DAConnectionPolicySettings {
    pub max_dispersal_failures: usize,
    pub max_sampling_failures: usize,
    pub max_replication_failures: usize,
    pub malicious_threshold: usize,
}

pub struct DAConnectionPolicy {
    settings: DAConnectionPolicySettings,
}

impl DAConnectionPolicy {
    pub fn new(settings: DAConnectionPolicySettings) -> Self {
        Self { settings }
    }
}

impl PeerHealthPolicy for DAConnectionPolicy {
    type PeerStats = PeerStats;

    fn is_peer_malicious(&self, stats: &Self::PeerStats) -> bool {
        stats.dispersal_failures_rate >= self.settings.malicious_threshold
            || stats.sampling_failures_rate >= self.settings.malicious_threshold
            || stats.replication_failures_rate >= self.settings.malicious_threshold
    }

    fn is_peer_unhealthy(&self, stats: &Self::PeerStats) -> bool {
        stats.dispersal_failures_rate >= self.settings.max_dispersal_failures
            || stats.sampling_failures_rate >= self.settings.max_sampling_failures
            || stats.replication_failures_rate >= self.settings.max_replication_failures
    }
}
