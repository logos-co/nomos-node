use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    time::Duration,
};

use fixed::types::U57F7;
use multiaddr::Multiaddr;
use nomos_mix_message::MixMessage;
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::membership::Membership;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ConnectionMaintenanceSettings {
    pub peering_degree: usize,
    pub max_peering_degree: usize,
    /// NOTE: We keep this optional until we gain confidence in parameter values that don't cause false detection.
    pub monitor: Option<ConnectionMonitorSettings>,
}

/// Connection maintenance to detect malicious and unhealthy peers
/// based on the number of messages sent by each peer in time windows
pub struct ConnectionMaintenance<M, R>
where
    M: MixMessage,
    R: RngCore,
{
    settings: ConnectionMaintenanceSettings,
    membership: Membership<M>,
    rng: R,
    connected_peers: HashSet<Multiaddr>,
    malicious_peers: HashSet<Multiaddr>,
    /// Monitors to measure the number of effective and drop messages sent by each peer
    /// NOTE: We keep this optional until we gain confidence in parameter values that don't cause false detection.
    monitors: Option<HashMap<Multiaddr, ConnectionMonitor>>,
}

impl<M, R> ConnectionMaintenance<M, R>
where
    M: MixMessage,
    M::PublicKey: PartialEq,
    R: RngCore,
{
    pub fn new(settings: ConnectionMaintenanceSettings, membership: Membership<M>, rng: R) -> Self {
        Self {
            settings,
            membership,
            rng,
            connected_peers: HashSet::new(),
            malicious_peers: HashSet::new(),
            monitors: settings.monitor.as_ref().map(|_| HashMap::new()),
        }
    }

    /// Choose the `peering_degree` number of remote nodes to connect to.
    pub fn bootstrap(&mut self) -> Vec<Multiaddr> {
        self.membership
            .choose_remote_nodes(&mut self.rng, self.settings.peering_degree)
            .iter()
            .map(|node| node.address.clone())
            .collect()
        // We don't add the peers to `connected_peers` because the dialings are not started yet.
    }

    /// Add a peer, which is fully connected, to the list of connected peers.
    pub fn add_connected_peer(&mut self, peer: Multiaddr) {
        self.connected_peers.insert(peer);
    }

    /// Remove a peer that has been disconnected.
    pub fn remove_connected_peer(&mut self, peer: &Multiaddr) {
        self.connected_peers.remove(peer);
    }

    /// Return the set of connected peers.
    pub fn connected_peers(&self) -> &HashSet<Multiaddr> {
        &self.connected_peers
    }

    /// Record a effective message sent by the [`peer`].
    /// If the peer was added during the current time window, the peer is not monitored
    /// until the next time window, to avoid false detection.
    pub fn record_effective_message(&mut self, peer: &Multiaddr) {
        if let Some(monitors) = self.monitors.as_mut() {
            if let Some(monitor) = monitors.get_mut(peer) {
                monitor.effective_messages = monitor
                    .effective_messages
                    .checked_add(U57F7::ONE)
                    .unwrap_or_else(|| {
                        tracing::warn!(
                            "Skipping recording an effective message due to overflow: Peer:{:?}",
                            peer
                        );
                        monitor.effective_messages
                    });
            }
        }
    }

    /// Record a drop message sent by the [`peer`].
    /// If the peer was added during the current time window, the peer is not monitored
    /// until the next time window, to avoid false detection.
    pub fn record_drop_message(&mut self, peer: &Multiaddr) {
        if let Some(monitors) = self.monitors.as_mut() {
            if let Some(monitor) = monitors.get_mut(peer) {
                monitor.drop_messages = monitor
                    .drop_messages
                    .checked_add(U57F7::ONE)
                    .unwrap_or_else(|| {
                        tracing::warn!(
                            "Skipping recording a drop message due to overflow: Peer:{:?}",
                            peer
                        );
                        monitor.drop_messages
                    });
            }
        }
    }

    /// Analyze connection monitors to identify malicious or unhealthy peers,
    /// and return which peers to disconnect from and which new peers to connect with.
    /// Additionally, this function resets and prepares the monitors for the next time window.
    pub fn reset(&mut self) -> (HashSet<Multiaddr>, HashSet<Multiaddr>) {
        let (malicious_peers, unhealthy_peers) = self.analyze_monitors();

        // Choose peers to connect with
        let peers_to_close = malicious_peers;
        let num_to_connect = peers_to_close.len() + unhealthy_peers.len();
        let num_to_connect = self.adjust_num_to_connect(num_to_connect, peers_to_close.len());
        let peers_to_connect = if num_to_connect > 0 {
            // Exclude connected peers and malicious peers from the candidate set
            let excludes = self
                .connected_peers
                .union(&self.malicious_peers)
                .cloned()
                .collect();
            self.membership
                .filter_and_choose_remote_nodes(&mut self.rng, num_to_connect, &excludes)
                .iter()
                .map(|node| node.address.clone())
                .collect()
        } else {
            HashSet::new()
        };

        self.reset_monitors();

        (peers_to_close, peers_to_connect)
    }

    /// Find malicious peers and unhealthy peers by analyzing connection monitors.
    /// The set of malicious peers is disjoint from the set of unhealthy peers.
    fn analyze_monitors(&mut self) -> (HashSet<Multiaddr>, HashSet<Multiaddr>) {
        let mut malicious_peers = HashSet::new();
        let mut unhealthy_peers = HashSet::new();

        if let Some(monitors) = self.monitors.as_mut() {
            let settings = &self.settings.monitor.unwrap();
            monitors.iter().for_each(|(peer, meter)| {
                // Skip peers that are disconnected during the current time window
                // because we have nothing to do for the connection that doesn't exist.
                if self.connected_peers.contains(peer) {
                    if meter.is_malicious(settings) {
                        tracing::warn!("Detected a malicious peer: {:?}", peer);
                        malicious_peers.insert(peer.clone());
                    } else if meter.is_unhealthy(settings) {
                        tracing::warn!("Detected an unhealthy peer: {:?}", peer);
                        unhealthy_peers.insert(peer.clone());
                    }
                }
            });
        }

        assert!(malicious_peers.is_disjoint(&unhealthy_peers));
        (malicious_peers, unhealthy_peers)
    }

    /// Reset monitors for all connected peers to be monitored in the next time window.
    /// To avoid false detection, we only monitor peers that were already connected
    /// before the time window started.
    fn reset_monitors(&mut self) {
        if let Some(monitors) = self.monitors.as_mut() {
            *monitors = self
                .connected_peers
                .iter()
                .cloned()
                .map(|peer| (peer, ConnectionMonitor::new()))
                .collect();
        }
    }

    /// Adjust the number of new connections to establish in order to not exceed `max_peering_degree`.
    fn adjust_num_to_connect(&self, num_to_connect: usize, num_to_close: usize) -> usize {
        let max_peering_degree = self.settings.max_peering_degree;
        let num_peers_after_close = self.connected_peers.len() - num_to_close;
        if num_peers_after_close + num_to_connect > max_peering_degree {
            let new_num_to_connect = max_peering_degree - num_peers_after_close;
            tracing::warn!(
                "Cannot establish {} new connections due to max_peering_degree:{}. Instead, establishing {} connections",
                num_to_connect, max_peering_degree, new_num_to_connect
            );
            new_num_to_connect
        } else {
            num_to_connect
        }
    }
}

/// Meter to count the number of effective and drop messages sent by a peer
#[derive(Debug)]
struct ConnectionMonitor {
    effective_messages: U57F7,
    drop_messages: U57F7,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ConnectionMonitorSettings {
    /// Time interval to measure/evaluate the number of messages sent by each peer.
    pub time_window: Duration,
    /// The number of effective (data or cover) messages that a peer is expected to send in a given time window.
    /// If the measured count is greater than (expected * (1 + tolerance)), the peer is considered malicious.
    /// If the measured count is less than (expected * (1 - tolerance)), the peer is considered unhealthy.
    pub expected_effective_messages: U57F7,
    pub effective_message_tolerance: U57F7,
    /// The number of drop messages that a peer is expected to send in a given time window.
    /// If the measured count is greater than (expected * (1 + tolerance)), the peer is considered malicious.
    /// If the measured count is less than (expected * (1 - tolerance)), the peer is considered unhealthy.
    pub expected_drop_messages: U57F7,
    pub drop_message_tolerance: U57F7,
}

impl ConnectionMonitor {
    fn new() -> Self {
        Self {
            effective_messages: U57F7::ZERO,
            drop_messages: U57F7::ZERO,
        }
    }

    /// Check if the peer is malicious based on the number of effective and drop messages sent
    fn is_malicious(&self, settings: &ConnectionMonitorSettings) -> bool {
        let effective_threshold = settings.expected_effective_messages
            * (U57F7::ONE + settings.effective_message_tolerance);
        let drop_threshold =
            settings.expected_drop_messages * (U57F7::ONE + settings.drop_message_tolerance);
        self.effective_messages > effective_threshold || self.drop_messages > drop_threshold
    }

    /// Check if the peer is unhealthy based on the number of effective and drop messages sent
    fn is_unhealthy(&self, settings: &ConnectionMonitorSettings) -> bool {
        let effective_threshold = settings.expected_effective_messages
            * (U57F7::ONE - settings.effective_message_tolerance);
        let drop_threshold =
            settings.expected_drop_messages * (U57F7::ONE - settings.drop_message_tolerance);
        effective_threshold > self.effective_messages || drop_threshold > self.drop_messages
    }
}

#[cfg(test)]
mod tests {
    use nomos_mix_message::mock::MockMixMessage;
    use rand::{rngs::ThreadRng, thread_rng};

    use crate::membership::Node;

    use super::*;

    #[test]
    fn meter() {
        let settings = ConnectionMonitorSettings {
            time_window: Duration::from_secs(1),
            expected_effective_messages: U57F7::from_num(2.0),
            effective_message_tolerance: U57F7::from_num(0.1),
            expected_drop_messages: U57F7::from_num(1.0),
            drop_message_tolerance: U57F7::from_num(0.0),
        };

        let monitor = ConnectionMonitor {
            effective_messages: U57F7::from_num(2),
            drop_messages: U57F7::from_num(1),
        };
        assert!(!monitor.is_malicious(&settings));
        assert!(!monitor.is_unhealthy(&settings));

        let monitor = ConnectionMonitor {
            effective_messages: U57F7::from_num(3),
            drop_messages: U57F7::from_num(1),
        };
        assert!(monitor.is_malicious(&settings));
        assert!(!monitor.is_unhealthy(&settings));

        let monitor = ConnectionMonitor {
            effective_messages: U57F7::from_num(1),
            drop_messages: U57F7::from_num(1),
        };
        assert!(!monitor.is_malicious(&settings));
        assert!(monitor.is_unhealthy(&settings));

        let monitor = ConnectionMonitor {
            effective_messages: U57F7::from_num(2),
            drop_messages: U57F7::from_num(2),
        };
        assert!(monitor.is_malicious(&settings));
        assert!(!monitor.is_unhealthy(&settings));

        let monitor = ConnectionMonitor {
            effective_messages: U57F7::from_num(2),
            drop_messages: U57F7::from_num(0),
        };
        assert!(!monitor.is_malicious(&settings));
        assert!(monitor.is_unhealthy(&settings));
    }

    #[test]
    fn malicious_and_unhealthy() {
        let mut maintenance = init_maintenance(
            ConnectionMaintenanceSettings {
                peering_degree: 3,
                max_peering_degree: 5,
                monitor: Some(ConnectionMonitorSettings {
                    time_window: Duration::from_secs(1),
                    expected_effective_messages: U57F7::from_num(2.0),
                    effective_message_tolerance: U57F7::from_num(0.1),
                    expected_drop_messages: U57F7::from_num(0.0),
                    drop_message_tolerance: U57F7::from_num(0.0),
                }),
            },
            10,
        );

        let peers = maintenance
            .connected_peers
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(peers.len(), 3);

        // Peer 0 sends 3 effective messages, more than expected
        maintenance.record_effective_message(&peers[0]);
        maintenance.record_effective_message(&peers[0]);
        maintenance.record_effective_message(&peers[0]);
        // Peer 1 sends 2 effective messages, as expected
        maintenance.record_effective_message(&peers[1]);
        maintenance.record_effective_message(&peers[1]);
        // Peer 2 sends 1 effective messages, less than expected
        maintenance.record_effective_message(&peers[2]);

        let (peers_to_close, peers_to_connect) = maintenance.reset();
        // Peer 0 is malicious
        assert_eq!(peers_to_close, HashSet::from_iter(vec![peers[0].clone()]));
        // Because Peer 1 is malicious and Peer 2 is unhealthy, 2 new connections should be established
        assert_eq!(peers_to_connect.len(), 2);
        assert!(peers_to_connect.is_disjoint(&maintenance.connected_peers));
    }

    #[test]
    fn exceed_max_peering_degree() {
        let mut maintenance = init_maintenance(
            ConnectionMaintenanceSettings {
                peering_degree: 3,
                max_peering_degree: 4,
                monitor: Some(ConnectionMonitorSettings {
                    time_window: Duration::from_secs(1),
                    expected_effective_messages: U57F7::from_num(2.0),
                    effective_message_tolerance: U57F7::from_num(0.1),
                    expected_drop_messages: U57F7::from_num(0.0),
                    drop_message_tolerance: U57F7::from_num(0.0),
                }),
            },
            10,
        );

        let peers = maintenance
            .connected_peers
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(peers.len(), 3);

        // Peer 0 sends 3 effective messages, more than expected
        maintenance.record_effective_message(&peers[0]);
        maintenance.record_effective_message(&peers[0]);
        maintenance.record_effective_message(&peers[0]);
        // Peer 1 and 2 send 1 effective messages, less than expected
        maintenance.record_effective_message(&peers[1]);
        maintenance.record_effective_message(&peers[2]);

        let (peers_to_close, peers_to_connect) = maintenance.reset();
        // Peer 0 is malicious
        assert_eq!(peers_to_close, HashSet::from_iter(vec![peers[0].clone()]));
        // Even though we detected 1 malicious and 2 unhealthy peers,
        // we cannot establish 3 new connections due to max_peering_degree.
        // Instead, 2 new connections should be established.
        assert_eq!(peers_to_connect.len(), 2);
        assert!(peers_to_connect.is_disjoint(&maintenance.connected_peers));
    }

    fn init_maintenance(
        settings: ConnectionMaintenanceSettings,
        node_count: usize,
    ) -> ConnectionMaintenance<MockMixMessage, ThreadRng> {
        let nodes = nodes(node_count);
        let mut maintenance = ConnectionMaintenance::<MockMixMessage, ThreadRng>::new(
            settings,
            Membership::new(nodes.clone(), nodes[0].public_key),
            thread_rng(),
        );
        (1..=settings.peering_degree).for_each(|i| {
            maintenance.add_connected_peer(nodes[i].address.clone());
        });
        let _ = maintenance.reset();
        maintenance
    }

    fn nodes(count: usize) -> Vec<Node<<MockMixMessage as MixMessage>::PublicKey>> {
        (0..count)
            .map(|i| Node {
                address: format!("/ip4/127.0.0.1/udp/{}/quic-v1", 1000 + i)
                    .parse()
                    .unwrap(),
                public_key: [i as u8; 32],
            })
            .collect()
    }
}
