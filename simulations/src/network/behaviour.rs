// std
use std::time::Duration;
// crates
use serde::{Deserialize, Serialize};
// internal

#[derive(Default, Serialize, Deserialize)]
pub struct NetworkBehaviour {
    pub delay: Duration,
    pub drop: f64,
}
