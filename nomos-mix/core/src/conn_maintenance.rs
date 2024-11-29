use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    hash::Hash,
    time::Duration,
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ConnectionMaintenanceSettings {
    pub time_window: Duration,
    /// The number of effective (data or cover) messages that a peer is expected to send in a given time window.
    /// If the measured count is greater than (expected * (1 + tolerance)), the peer is considered malicious.
    /// If the measured count is less than (expected * (1 - tolerance)), the peer is considered unhealthy.
    pub expected_effective_messages: f32,
    pub effective_message_tolerance: f32,
    /// The number of drop messages that a peer is expected to send in a given time window.
    /// If the measured count is greater than (expected * (1 + tolerance)), the peer is considered malicious.
    /// If the measured count is less than (expected * (1 - tolerance)), the peer is considered unhealthy.
    pub expected_drop_messages: f32,
    pub drop_message_tolerance: f32,
}

pub struct ConnectionMaintenance<Peer> {
    settings: ConnectionMaintenanceSettings,
    meters: HashMap<Peer, ConnectionMeter>,
}

impl<Peer> ConnectionMaintenance<Peer>
where
    Peer: Debug + Eq + Hash + Clone,
{
    pub fn new(settings: ConnectionMaintenanceSettings) -> Self {
        Self {
            settings,
            meters: HashMap::new(),
        }
    }

    pub fn add_effective(&mut self, peer: Peer) {
        self.meter(peer).effective_messages += 1;
    }

    pub fn add_drop(&mut self, peer: Peer) {
        self.meter(peer).drop_messages += 1;
    }

    fn meter(&mut self, peer: Peer) -> &mut ConnectionMeter {
        self.meters.entry(peer).or_insert_with(ConnectionMeter::new)
    }

    pub fn reset(&mut self) -> (HashSet<Peer>, HashSet<Peer>) {
        let mut malicious_peers = HashSet::new();
        let mut unhealthy_peers = HashSet::new();

        self.meters.iter().for_each(|(peer, meter)| {
            if meter.is_malicious(&self.settings) {
                malicious_peers.insert(peer.clone());
            } else if meter.is_unhealthy(&self.settings) {
                unhealthy_peers.insert(peer.clone());
            }
        });
        self.meters.clear();

        (malicious_peers, unhealthy_peers)
    }
}

#[derive(Debug)]
struct ConnectionMeter {
    effective_messages: usize,
    drop_messages: usize,
}

impl ConnectionMeter {
    fn new() -> Self {
        Self {
            effective_messages: 0,
            drop_messages: 0,
        }
    }

    fn is_malicious(&self, settings: &ConnectionMaintenanceSettings) -> bool {
        let effective_threshold =
            settings.expected_effective_messages * (1.0 + settings.effective_message_tolerance);
        let drop_threshold =
            settings.expected_drop_messages * (1.0 + settings.drop_message_tolerance);
        self.effective_messages as f32 > effective_threshold
            || self.drop_messages as f32 > drop_threshold
    }

    fn is_unhealthy(&self, settings: &ConnectionMaintenanceSettings) -> bool {
        let effective_threshold =
            settings.expected_effective_messages * (1.0 - settings.effective_message_tolerance);
        let drop_threshold =
            settings.expected_drop_messages * (1.0 - settings.drop_message_tolerance);
        effective_threshold > self.effective_messages as f32
            || drop_threshold > self.drop_messages as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malicious_and_unhealthy_by_effective() {
        let settings = ConnectionMaintenanceSettings {
            time_window: Duration::from_secs(1),
            expected_effective_messages: 2.0,
            effective_message_tolerance: 0.1,
            expected_drop_messages: 0.0,
            drop_message_tolerance: 0.0,
        };
        let mut maintenance = ConnectionMaintenance::<u8>::new(settings);
        // Peer 0 sends 3 effective messages, more than expected
        maintenance.add_effective(0);
        maintenance.add_effective(0);
        maintenance.add_effective(0);
        // Peer 1 sends 2 effective messages, as expected
        maintenance.add_effective(1);
        maintenance.add_effective(1);
        // Peer 2 sends 1 effective messages, less than expected
        maintenance.add_effective(2);

        let (malicious, unhealthy) = maintenance.reset();
        assert_eq!(malicious, HashSet::from_iter(vec![0]));
        assert_eq!(unhealthy, HashSet::from_iter(vec![2]));
    }

    #[test]
    fn malicious_and_unhealthy_by_drop() {
        let settings = ConnectionMaintenanceSettings {
            time_window: Duration::from_secs(1),
            expected_effective_messages: 0.0,
            effective_message_tolerance: 0.0,
            expected_drop_messages: 2.0,
            drop_message_tolerance: 0.1,
        };
        let mut maintenance = ConnectionMaintenance::<u8>::new(settings);
        // Peer 0 sends 3 drop messages, more than expected
        maintenance.add_drop(0);
        maintenance.add_drop(0);
        maintenance.add_drop(0);
        // Peer 1 sends 2 drop messages, as expected
        maintenance.add_drop(1);
        maintenance.add_drop(1);
        // Peer 2 sends 1 drop messages, less than expected
        maintenance.add_drop(2);

        let (malicious, unhealthy) = maintenance.reset();
        assert_eq!(malicious, HashSet::from_iter(vec![0]));
        assert_eq!(unhealthy, HashSet::from_iter(vec![2]));
    }
}
