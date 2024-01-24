use super::types::*;

mod branch_overlay;
mod flat_overlay;
mod leadership;
mod membership;
mod random_beacon;
mod threshold;
mod tree_overlay;

pub use branch_overlay::*;
pub use flat_overlay::*;
pub use leadership::*;
pub use membership::*;
pub use random_beacon::*;
pub use tree_overlay::*;

use std::marker::Send;

pub trait Overlay: Clone {
    type Settings: Clone + Send + Sync + 'static;
    type LeaderSelection: LeaderSelection + Clone + Send + Sync + 'static;
    type CommitteeMembership: CommitteeMembership + Clone + Send + Sync + 'static;

    fn new(settings: Self::Settings) -> Self;
    fn root_committee(&self) -> Committee;
    fn is_member_of_child_committee(&self, parent: NodeId, child: NodeId) -> bool;
    fn is_member_of_root_committee(&self, id: NodeId) -> bool;
    fn is_member_of_leaf_committee(&self, id: NodeId) -> bool;
    fn is_child_of_root_committee(&self, id: NodeId) -> bool;
    fn parent_committee(&self, id: NodeId) -> Option<Committee>;
    fn child_committees(&self, id: NodeId) -> Vec<Committee>;
    fn leaf_committees(&self, id: NodeId) -> Vec<Committee>;
    fn node_committee(&self, id: NodeId) -> Committee;
    fn next_leader(&self) -> NodeId;
    fn super_majority_threshold(&self, id: NodeId) -> usize;
    fn leader_super_majority_threshold(&self, id: NodeId) -> usize;
    fn update_leader_selection<F, E>(&self, f: F) -> Result<Self, E>
    where
        F: FnOnce(Self::LeaderSelection) -> Result<Self::LeaderSelection, E>;
    fn update_committees<F, E>(&self, f: F) -> Result<Self, E>
    where
        F: FnOnce(Self::CommitteeMembership) -> Result<Self::CommitteeMembership, E>;
}

pub trait LeaderSelection: Clone {
    fn next_leader(&self, nodes: &[NodeId]) -> NodeId;
}

pub trait CommitteeMembership: Clone {
    fn reshape_committees(&self, nodes: &mut [NodeId]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::{FisherYatesShuffle, RoundRobin};

    const ENTROPY: [u8; 32] = [0; 32];

    fn overlay_fns_match(a: &impl Overlay, b: &impl Overlay, nodes: &[NodeId]) {
        assert_eq!(a.root_committee(), b.root_committee());
        assert_eq!(
            a.is_member_of_child_committee(nodes[0], nodes[1]),
            b.is_member_of_child_committee(nodes[0], nodes[1]),
        );
        assert_eq!(
            a.is_member_of_root_committee(nodes[0]),
            b.is_member_of_root_committee(nodes[0]),
        );
        assert_eq!(
            a.is_member_of_leaf_committee(nodes[0]),
            b.is_member_of_leaf_committee(nodes[0]),
        );
        assert_eq!(
            a.is_child_of_root_committee(nodes[0]),
            b.is_child_of_root_committee(nodes[0])
        );
        assert_eq!(a.parent_committee(nodes[0]), b.parent_committee(nodes[0]));
        assert_eq!(a.child_committees(nodes[0]), b.child_committees(nodes[0]));
        assert_eq!(a.leaf_committees(nodes[0]), b.leaf_committees(nodes[0]));
        assert_eq!(a.node_committee(nodes[0]), b.node_committee(nodes[0]));
        assert_eq!(
            a.super_majority_threshold(nodes[0]),
            b.super_majority_threshold(nodes[0])
        );
        assert_eq!(
            a.leader_super_majority_threshold(nodes[0]),
            b.leader_super_majority_threshold(nodes[0])
        );
    }

    #[test]
    fn compare_tree_branch_one_committee() {
        let nodes: Vec<_> = (0..10).map(|i| NodeId::new([i as u8; 32])).collect();
        let tree_overlay = TreeOverlay::new(TreeOverlaySettings {
            nodes: nodes.clone(),
            current_leader: nodes[0],
            number_of_committees: 1,
            leader: RoundRobin::new(),
            committee_membership: FisherYatesShuffle::new(ENTROPY),
            super_majority_threshold: None,
        });
        let branch_overlay = BranchOverlay::new(BranchOverlaySettings {
            current_leader: nodes[0],
            nodes: nodes.clone(),
            branch_depth: 1,
            leader: RoundRobin::new(),
            committee_membership: FisherYatesShuffle::new(ENTROPY),
        });

        overlay_fns_match(&tree_overlay, &branch_overlay, &nodes);
    }
}
