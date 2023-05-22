pub mod happy;
pub mod unhappy;

// std
use std::collections::HashSet;

// crates
use serde::{Deserialize, Serialize};

// internal
use consensus_engine::NodeId;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CarnotTallySettings {
    pub threshold: usize,
    // TODO: this probably should be dynamic and should change with the view (?)
    pub participating_nodes: HashSet<NodeId>,
}
