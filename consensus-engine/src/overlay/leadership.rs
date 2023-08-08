// std

// crates
use serde::{Deserialize, Serialize};

// internal
use crate::overlay::LeaderSelection;
use crate::NodeId;

#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RoundRobin {
    cur: usize,
}

impl RoundRobin {
    pub fn new() -> Self {
        Self { cur: 0 }
    }

    pub fn advance(&self) -> Self {
        Self {
            cur: (self.cur + 1),
        }
    }
}

impl LeaderSelection for RoundRobin {
    fn next_leader(&self, nodes: &[NodeId]) -> NodeId {
        nodes[self.cur % nodes.len()]
    }
}
