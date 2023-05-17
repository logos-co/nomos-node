use consensus_engine::{Committee, NodeId, Overlay, View};

#[derive(Clone, Debug)]
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
        Committee::new()
    }

    fn node_committee(&self, _id: NodeId) -> consensus_engine::Committee {
        self.nodes.clone().into_iter().collect()
    }

    fn child_committees(&self, _id: NodeId) -> Vec<consensus_engine::Committee> {
        vec![]
    }

    fn leaf_committees(&self, _id: NodeId) -> Vec<consensus_engine::Committee> {
        vec![self.root_committee()]
    }

    fn leader(&self, view: View) -> NodeId {
        self.nodes[view as usize % self.nodes.len()]
    }

    fn super_majority_threshold(&self, _id: NodeId) -> usize {
        0
    }

    fn leader_super_majority_threshold(&self, _id: NodeId) -> usize {
        self.nodes.len() * 2 / 3 + 1
    }
}
