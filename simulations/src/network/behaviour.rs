// std
use std::{collections::HashMap, time::Duration};
// crates
use rand::Rng;
use serde::{Deserialize, Serialize};

use super::{NetworkBehaviourKey, NetworkSettings};
// internal

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct NetworkBehaviour {
    pub delay: Duration,
    pub drop: f64,
}

impl NetworkBehaviour {
    pub fn new(delay: Duration, drop: f64) -> Self {
        Self { delay, drop }
    }

    pub fn delay(&self) -> Duration {
        self.delay
    }

    pub fn should_drop<R: Rng>(&self, rng: &mut R) -> bool {
        rng.gen_bool(self.drop)
    }
}

// Takes a reference to the simulation_settings and returns a HashMap representing the
// network behaviors for pairs of NodeIds.
pub fn create_behaviours(
    network_settings: &NetworkSettings,
) -> HashMap<NetworkBehaviourKey, NetworkBehaviour> {
    network_settings
        .network_behaviors
        .iter()
        .map(|(k, d)| (*k, NetworkBehaviour::new(*d, 0.0)))
        .collect()
}
