use consensus_engine::{NodeId, Overlay, View};

#[derive(Clone)]
/// Flat overlay with a single committee and round robin leader selection.
pub struct FlatRoundRobin {
    nodes: Vec<NodeId>,
}

impl Overlay for FlatRoundRobin {
    fn new(nodes: Vec<NodeId>) -> Self {
        Self { nodes }
    }

    fn root_committee(&self) -> consensus_engine::Committee {
        self.nodes.clone().into_iter().collect()
    }

    fn rebuild(&mut self, _timeout_qc: consensus_engine::TimeoutQc) {
        todo!()
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

    fn parent_committee(&self, _id: NodeId) -> consensus_engine::Committee {
        panic!("root committee does not have a parent committee")
    }

    fn leaf_committees(&self, _id: NodeId) -> Vec<consensus_engine::Committee> {
        [self.root_committee()].into_iter().collect()
    }

    fn leader(&self, view: View) -> NodeId {
        self.nodes[view as usize % self.nodes.len()]
    }

    fn super_majority_threshold(&self, _id: NodeId) -> usize {
        self.nodes.len() * 3 / 2 + 1
    }

    fn leader_super_majority_threshold(&self, id: NodeId) -> usize {
        self.super_majority_threshold(id)
    }
}
