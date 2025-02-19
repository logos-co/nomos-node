// std
use std::collections::HashMap;
use std::time::{Duration, Instant};
// crates
use fixed::types::U57F7;
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
// internal
use crate::maintenance::monitor::{ConnectionMonitor, ConnectionMonitorOutput, PeerStatus};
use crate::protocols::dispersal::executor::behaviour::{
    DispersalError as ExecutorDispersalError, DispersalExecutorEvent,
};
use crate::protocols::dispersal::validator::behaviour::{
    DispersalError as ValidatorDispersalError, DispersalEvent as DispersalValidatorEvent,
};
use crate::protocols::replication::behaviour::{ReplicationError, ReplicationEvent};
use crate::protocols::sampling::behaviour::{SamplingError, SamplingEvent};

pub enum MonitorEvent {
    ExecutorDispersal(ExecutorDispersalError),
    ValidatorDispersal(ValidatorDispersalError),
    Replication(ReplicationError),
    Sampling(SamplingError),
    Noop,
}

impl MonitorEvent {
    pub fn peer_id(&self) -> Option<&PeerId> {
        match self {
            MonitorEvent::ExecutorDispersal(dispersal_error) => dispersal_error.peer_id(),
            MonitorEvent::ValidatorDispersal(dispersal_error) => dispersal_error.peer_id(),
            MonitorEvent::Replication(replication_error) => replication_error.peer_id(),
            MonitorEvent::Sampling(sampling_error) => sampling_error.peer_id(),
            MonitorEvent::Noop => None,
        }
    }
}

impl From<&DispersalExecutorEvent> for MonitorEvent {
    fn from(event: &DispersalExecutorEvent) -> Self {
        match event {
            DispersalExecutorEvent::DispersalSuccess { .. } => MonitorEvent::Noop,
            DispersalExecutorEvent::DispersalError { error } => {
                MonitorEvent::ExecutorDispersal(error.clone())
            }
        }
    }
}

impl From<&DispersalValidatorEvent> for MonitorEvent {
    fn from(event: &DispersalValidatorEvent) -> Self {
        match event {
            DispersalValidatorEvent::IncomingMessage { .. } => MonitorEvent::Noop,
            DispersalValidatorEvent::DispersalError { error } => {
                MonitorEvent::ValidatorDispersal(error.clone())
            }
        }
    }
}

impl From<&ReplicationEvent> for MonitorEvent {
    fn from(event: &ReplicationEvent) -> Self {
        match event {
            ReplicationEvent::IncomingMessage { .. } => MonitorEvent::Noop,
            ReplicationEvent::ReplicationError { error } => {
                MonitorEvent::Replication(error.clone())
            }
        }
    }
}

impl From<&SamplingEvent> for MonitorEvent {
    fn from(event: &SamplingEvent) -> Self {
        match event {
            SamplingEvent::SamplingSuccess { .. } => MonitorEvent::Noop,
            SamplingEvent::IncomingSample { .. } => MonitorEvent::Noop,
            SamplingEvent::SamplingError { error } => MonitorEvent::Sampling(error.clone()),
        }
    }
}

/// Tracks failure rates for different protocols using exponential weighted moving average.
#[derive(Default, Debug)]
pub struct PeerStats {
    // Calculated using EWMA to give more weight to recent failures while gradually decaying over time.
    pub dispersal_failures_rate: U57F7,
    pub sampling_failures_rate: U57F7,
    pub replication_failures_rate: U57F7,

    // Track the time of the last failure for decay calculations.
    last_dispersal_failure: Option<Instant>,
    last_sampling_failure: Option<Instant>,
    last_replication_failure: Option<Instant>,
}

impl PeerStats {
    pub fn compute_dispersal_failure_rate(
        &self,
        now: Instant,
        window: Duration,
        factor: U57F7,
    ) -> U57F7 {
        compute_failure_rate(
            now,
            self.last_dispersal_failure.unwrap_or(now),
            self.dispersal_failures_rate,
            window,
            factor,
        )
    }

    pub fn compute_sampling_failure_rate(
        &self,
        now: Instant,
        window: Duration,
        factor: U57F7,
    ) -> U57F7 {
        compute_failure_rate(
            now,
            self.last_sampling_failure.unwrap_or(now),
            self.sampling_failures_rate,
            window,
            factor,
        )
    }

    /// **Updates the replication failure rate**
    pub fn compute_replication_failure_rate(
        &self,
        now: Instant,
        window: Duration,
        factor: U57F7,
    ) -> U57F7 {
        compute_failure_rate(
            now,
            self.last_replication_failure.unwrap_or(now),
            self.replication_failures_rate,
            window,
            factor,
        )
    }

    pub fn get_updated_stats(
        &self,
        now: Instant,
        time_window: Duration,
        decay_factor: U57F7,
    ) -> Self {
        Self {
            dispersal_failures_rate: self.compute_dispersal_failure_rate(
                now,
                time_window,
                decay_factor,
            ),
            sampling_failures_rate: self.compute_sampling_failure_rate(
                now,
                time_window,
                decay_factor,
            ),
            replication_failures_rate: self.compute_replication_failure_rate(
                now,
                time_window,
                decay_factor,
            ),
            last_dispersal_failure: self.last_dispersal_failure,
            last_sampling_failure: self.last_sampling_failure,
            last_replication_failure: self.last_replication_failure,
        }
    }
}

pub trait PeerHealthPolicy {
    type PeerStats;

    /// Evaluates whether a peer is malicious.
    ///
    /// Returns `true` if the peer is deemed malicious, otherwise `false`.
    fn is_peer_malicious(&self, stats: &Self::PeerStats) -> bool;

    /// Evaluates whether a peer is unhealthy.
    ///
    /// Returns `true` if the peer is deemed unhealthy, otherwise `false`.
    fn is_peer_unhealthy(&self, stats: &Self::PeerStats) -> bool;
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DAConnectionMonitorSettings {
    pub failure_time_window: Duration,
    pub time_decay_factor: U57F7,
}

pub struct DAConnectionMonitor<Policy> {
    peer_stats: HashMap<PeerId, PeerStats>,
    policy: Policy,
    settings: DAConnectionMonitorSettings,
}

impl<Policy> DAConnectionMonitor<Policy>
where
    Policy: PeerHealthPolicy<PeerStats = PeerStats>,
{
    pub fn new(settings: DAConnectionMonitorSettings, policy: Policy) -> Self {
        Self {
            peer_stats: HashMap::new(),
            policy,
            settings,
        }
    }

    fn evaluate_peer(&self, now: Instant, peer_id: &PeerId) -> PeerStatus {
        if let Some(stats) = self.peer_stats.get(peer_id) {
            // We need to recompute the failure rate upon the evaluation as time has moved on and
            // the failure rate must have decayed.
            let stats = stats.get_updated_stats(
                now,
                self.settings.failure_time_window,
                self.settings.time_decay_factor,
            );

            if self.policy.is_peer_malicious(&stats) {
                return PeerStatus::Malicious;
            }

            if self.policy.is_peer_unhealthy(&stats) {
                return PeerStatus::Unhealthy;
            }

            PeerStatus::Healthy
        } else {
            PeerStatus::Healthy
        }
    }
}

impl<Policy> ConnectionMonitor for DAConnectionMonitor<Policy>
where
    Policy: PeerHealthPolicy<PeerStats = PeerStats>,
{
    type Event = MonitorEvent;

    fn record_event(&mut self, event: Self::Event) -> Option<ConnectionMonitorOutput> {
        if let Some(peer_id) = event.peer_id() {
            let stats = self.peer_stats.entry(*peer_id).or_default();
            let now = Instant::now();
            match event {
                MonitorEvent::ExecutorDispersal(_) | MonitorEvent::ValidatorDispersal(_) => {
                    stats.dispersal_failures_rate = stats.compute_dispersal_failure_rate(
                        now,
                        self.settings.failure_time_window,
                        self.settings.time_decay_factor,
                    ) + U57F7::ONE; // Compute updated rate and increment by one because its a new error.
                    stats.last_dispersal_failure = Some(now);
                }
                MonitorEvent::Replication(_) => {
                    stats.replication_failures_rate = stats.compute_replication_failure_rate(
                        now,
                        self.settings.failure_time_window,
                        self.settings.time_decay_factor,
                    ) + U57F7::ONE; // Compute updated rate and increment by one because its a new error.
                    stats.last_replication_failure = Some(now);
                }
                MonitorEvent::Sampling(_) => {
                    stats.sampling_failures_rate = stats.compute_sampling_failure_rate(
                        now,
                        self.settings.failure_time_window,
                        self.settings.time_decay_factor,
                    ) + U57F7::ONE; // Compute updated rate and increment by one because its a new error.
                    stats.last_sampling_failure = Some(now);
                }
                MonitorEvent::Noop => {}
            };

            Some(ConnectionMonitorOutput {
                peer_id: *peer_id,
                peer_status: self.evaluate_peer(now, peer_id),
            })
        } else {
            None
        }
    }

    fn reset_peer(&mut self, peer_id: &PeerId) {
        self.peer_stats.remove(peer_id);
    }
}

fn compute_failure_rate(
    now: Instant,
    last_failure: Instant,
    failure_rate: U57F7,
    failure_time_window: Duration,
    decay_factor: U57F7,
) -> U57F7 {
    let elapsed = now.duration_since(last_failure).as_secs_f64();
    let mut new_failure_rate = failure_rate;

    // Apply exponential decay to the failure rate
    if elapsed > 0.0 {
        let time_based_decay = (-elapsed / failure_time_window.as_secs_f64()).exp();
        let decay_factor_fixed = U57F7::from_num(time_based_decay);
        new_failure_rate *= decay_factor_fixed * decay_factor;
    }

    new_failure_rate
}

#[cfg(test)]
mod tests {
    use libp2p::PeerId;
    use std::time::Duration;

    use super::*;
    use crate::swarm::{common::policy::DAConnectionPolicy, DAConnectionPolicySettings};

    fn setup_monitor() -> DAConnectionMonitor<DAConnectionPolicy> {
        let monitor_settings = DAConnectionMonitorSettings {
            failure_time_window: Duration::from_secs(10),
            time_decay_factor: U57F7::lit("0.8"),
        };
        let policy_settings = DAConnectionPolicySettings {
            max_dispersal_failures: 2,
            max_sampling_failures: 2,
            max_replication_failures: 2,
            malicious_threshold: 3,
        };
        DAConnectionMonitor::new(monitor_settings, DAConnectionPolicy::new(policy_settings))
    }

    #[test]
    fn test_peer_starts_healthy() {
        let monitor = setup_monitor();
        let peer_id = PeerId::random();

        assert_eq!(
            monitor.evaluate_peer(Instant::now(), &peer_id),
            PeerStatus::Healthy
        );
    }

    #[test]
    fn test_peer_becomes_unhealthy() {
        let mut monitor = setup_monitor();
        let peer_id = PeerId::random();

        for _ in 0..4 {
            monitor.record_event(MonitorEvent::Sampling(SamplingError::Io {
                peer_id,
                error: std::io::Error::new(std::io::ErrorKind::Other, "Simulated I/O error"),
            }));
        }

        assert_eq!(
            monitor.evaluate_peer(Instant::now(), &peer_id),
            PeerStatus::Unhealthy
        );
    }

    #[test]
    fn test_peer_becomes_malicious() {
        let mut monitor = setup_monitor();
        let peer_id = PeerId::random();

        for _ in 0..100 {
            monitor.record_event(MonitorEvent::Sampling(SamplingError::Io {
                peer_id,
                error: std::io::Error::new(std::io::ErrorKind::Other, "Simulated I/O error"),
            }));
        }

        assert_eq!(
            monitor.evaluate_peer(Instant::now(), &peer_id),
            PeerStatus::Malicious
        );
    }

    #[test]
    fn test_failure_decay_over_time() {
        let mut monitor = setup_monitor();
        let peer_id = PeerId::random();

        for _ in 0..4 {
            monitor.record_event(MonitorEvent::Sampling(SamplingError::Io {
                peer_id,
                error: std::io::Error::new(std::io::ErrorKind::Other, "Simulated I/O error"),
            }));
        }

        assert_eq!(
            monitor.evaluate_peer(Instant::now(), &peer_id),
            PeerStatus::Unhealthy
        );

        // Simulate evaluation after 10 seconds waiting, failure rate should decay,
        // making the peer Healthy again.
        let later = Instant::now() + Duration::from_secs(10);
        assert_eq!(monitor.evaluate_peer(later, &peer_id), PeerStatus::Healthy);
    }

    #[test]
    fn test_peer_reset() {
        let mut monitor = setup_monitor();
        let peer_id = PeerId::random();

        for _ in 0..4 {
            monitor.record_event(MonitorEvent::Sampling(SamplingError::Io {
                peer_id,
                error: std::io::Error::new(std::io::ErrorKind::Other, "Simulated I/O error"),
            }));
        }

        assert_eq!(
            monitor.evaluate_peer(Instant::now(), &peer_id),
            PeerStatus::Unhealthy
        );

        monitor.reset_peer(&peer_id);

        assert_eq!(
            monitor.evaluate_peer(Instant::now(), &peer_id),
            PeerStatus::Healthy
        );
    }
}
