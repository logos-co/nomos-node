pub struct DAConnectionPolicySettings {
    pub max_dispersal_failures: usize,
    pub max_sampling_failures: usize,
    pub max_replication_failures: usize,
    pub malicious_threshold: usize,
}

pub struct DAConnectionPolicy {
    settings: DAConnectionPolicySettings,
}

impl PeerHealthPolicy for DAConnectionPolicy {
    type PeerStats = PeerStats;

    fn is_peer_malicious(&self, stats: &Self::PeerStats) -> bool {
        dispersal_rate >= self.settings.malicious_threshold
            || sampling_rate >= self.settings.malicious_threshold
            || replication_rate >= self.settings.malicious_threshold
    }

    fn is_peer_unhealthy(&self, stats: &Self::PeerStats) -> bool {
        dispersal_rate >= self.settings.max_dispersal_failures
            || sampling_rate >= self.settings.max_sampling_failures
            || replication_rate >= self.settings.max_replication_failures
    }
}
