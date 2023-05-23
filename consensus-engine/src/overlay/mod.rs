use super::types::*;

mod flat_round_robin;
pub use flat_round_robin::FlatRoundRobin;

pub trait Overlay: Clone {
    fn new(nodes: Vec<NodeId>) -> Self;
    fn root_committee(&self) -> Committee;
    fn rebuild(&mut self, timeout_qc: TimeoutQc);
    fn is_member_of_child_committee(&self, parent: NodeId, child: NodeId) -> bool;
    fn is_member_of_root_committee(&self, id: NodeId) -> bool;
    fn is_member_of_leaf_committee(&self, id: NodeId) -> bool;
    fn is_child_of_root_committee(&self, id: NodeId) -> bool;
    fn parent_committee(&self, id: NodeId) -> Committee;
    fn child_committees(&self, id: NodeId) -> Vec<Committee>;
    fn leaf_committees(&self, id: NodeId) -> Vec<Committee>;
    fn node_committee(&self, id: NodeId) -> Committee;
    fn leader(&self, view: View) -> NodeId;
    fn super_majority_threshold(&self, id: NodeId) -> usize;
    fn leader_super_majority_threshold(&self, id: NodeId) -> usize;
}
