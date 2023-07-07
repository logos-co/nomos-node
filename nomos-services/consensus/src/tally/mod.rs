pub mod happy;
pub mod timeout;
pub mod unhappy;

// std

// crates
use serde::{Deserialize, Serialize};

// internal
use consensus_engine::Committee;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CarnotTallySettings {
    pub threshold: usize,
    // TODO: this probably should be dynamic and should change with the view (?)
    pub participating_nodes: Committee,
}
