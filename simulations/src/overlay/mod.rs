pub mod flat;
pub mod tree;

// std
use std::collections::{BTreeSet, HashMap};
// crates
use rand::Rng;
// internal
use crate::node::{carnot::CarnotRole, CommitteeId, Node, NodeId};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Committee {
    pub nodes: BTreeSet<NodeId>,
    pub role: CarnotRole,
}

impl Committee {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.len() == 0
    }
}

pub type Leaders = BTreeSet<NodeId>;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Layout {
    pub committees: HashMap<CommitteeId, Committee>,
    pub from_committee: HashMap<NodeId, CommitteeId>,
    pub parent: HashMap<CommitteeId, CommitteeId>,
    pub children: HashMap<CommitteeId, Vec<CommitteeId>>,
    pub layers: HashMap<CommitteeId, Vec<CommitteeId>>,
}

impl Layout {
    pub fn new(
        committees: HashMap<CommitteeId, Committee>,
        parent: HashMap<CommitteeId, CommitteeId>,
        children: HashMap<CommitteeId, Vec<CommitteeId>>,
        layers: HashMap<CommitteeId, Vec<CommitteeId>>,
    ) -> Self {
        let from_committee = committees
            .iter()
            .flat_map(|(&committee_id, committee)| {
                committee
                    .nodes
                    .iter()
                    .map(move |&node_id| (node_id, committee_id))
            })
            .collect();
        Self {
            committees,
            from_committee,
            parent,
            children,
            layers,
        }
    }

    pub fn committee(&self, node_id: NodeId) -> CommitteeId {
        self.from_committee.get(&node_id).copied().unwrap()
    }

    pub fn committee_nodes(&self, committee_id: CommitteeId) -> &Committee {
        &self.committees[&committee_id]
    }

    pub fn parent(&self, committee_id: CommitteeId) -> Option<CommitteeId> {
        self.parent.get(&committee_id).copied()
    }

    pub fn parent_nodes(&self, committee_id: CommitteeId) -> Option<Committee> {
        self.parent(committee_id)
            .map(|c| self.committees[&c].clone())
    }

    pub fn children(&self, committee_id: CommitteeId) -> &[CommitteeId] {
        &self.children[&committee_id]
    }

    pub fn children_nodes(&self, committee_id: CommitteeId) -> Vec<&Committee> {
        self.children(committee_id)
            .iter()
            .map(|&committee_id| &self.committees[&committee_id])
            .collect()
    }

    pub fn node_ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.from_committee.keys().copied()
    }
}

pub trait Overlay<N: Node> {
    type Settings;

    fn new(settings: Self::Settings) -> Self;
    fn nodes(&self) -> Vec<NodeId>;
    fn leaders<R: Rng>(
        &self,
        nodes: &[NodeId],
        size: usize,
        rng: &mut R,
    ) -> Box<dyn Iterator<Item = NodeId>>;
    fn layout<R: Rng>(&self, nodes: &[NodeId], rng: &mut R) -> Layout;
}
