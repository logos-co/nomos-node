use super::LeaderSelection;
use crate::{Committee, NodeId, Overlay};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq)]
/// Flat overlay with a single committee and round robin leader selection.
pub struct FlatOverlay<L: LeaderSelection> {
    nodes: Vec<NodeId>,
    leader: L,
}

impl<L> Overlay for FlatOverlay<L>
where
    L: LeaderSelection + Send + Sync + 'static,
{
    type Settings = Settings<L>;
    type LeaderSelection = L;

    fn new(Settings { leader, nodes }: Self::Settings) -> Self {
        Self { nodes, leader }
    }

    fn root_committee(&self) -> crate::Committee {
        self.nodes.clone().into_iter().collect()
    }

    fn rebuild(&mut self, _timeout_qc: crate::TimeoutQc) {
        // do nothing for now
    }

    fn is_member_of_child_committee(&self, _parent: NodeId, _child: NodeId) -> bool {
        false
    }

    fn is_member_of_root_committee(&self, _id: NodeId) -> bool {
        true
    }

    fn is_member_of_leaf_committee(&self, _id: NodeId) -> bool {
        true
    }

    fn is_child_of_root_committee(&self, _id: NodeId) -> bool {
        false
    }

    fn parent_committee(&self, _id: NodeId) -> crate::Committee {
        Committee::new()
    }

    fn node_committee(&self, _id: NodeId) -> crate::Committee {
        self.nodes.clone().into_iter().collect()
    }

    fn child_committees(&self, _id: NodeId) -> Vec<crate::Committee> {
        vec![]
    }

    fn leaf_committees(&self, _id: NodeId) -> Vec<crate::Committee> {
        vec![self.root_committee()]
    }

    fn next_leader(&self) -> NodeId {
        self.leader.next_leader(&self.nodes)
    }

    fn super_majority_threshold(&self, _id: NodeId) -> usize {
        0
    }

    fn leader_super_majority_threshold(&self, _id: NodeId) -> usize {
        self.nodes.len() * 2 / 3 + 1
    }

    fn update_leader_selection<F, E>(&self, f: F) -> Result<Self, E>
    where
        F: FnOnce(Self::LeaderSelection) -> Result<Self::LeaderSelection, E>,
    {
        match f(self.leader.clone()) {
            Ok(leader_selection) => Ok(Self {
                leader: leader_selection,
                ..self.clone()
            }),
            Err(e) => Err(e),
        }
    }
}

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

#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Settings<L> {
    pub nodes: Vec<NodeId>,
    pub leader: L,
}
