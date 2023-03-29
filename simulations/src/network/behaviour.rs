// std
use std::time::Duration;
// crates
use rand::Rng;
use serde::{Deserialize, Serialize};
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
