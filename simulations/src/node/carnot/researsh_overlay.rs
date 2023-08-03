use std::marker::PhantomData;

use consensus_engine::{
    overlay::{CommitteeMembership, LeaderSelection},
    Committee, NodeId, TimeoutQc,
};

use crate::node::CommitteeExt;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct ResearchOverlaySettings<L: LeaderSelection> {
    num_nodes: usize,
    leader: L,
    leader_threshold: usize,
}

#[derive(Debug, Clone)]
pub struct ResearchOverlay<L: LeaderSelection, M: CommitteeMembership> {
    committee: Committee,
    nodes: Vec<NodeId>,
    l: L,
    leader_threshold: usize,
    _committee_membership: PhantomData<M>,
}

impl<L, M> consensus_engine::Overlay for ResearchOverlay<L, M>
where
    L: LeaderSelection + Send + Sync + 'static,
    M: CommitteeMembership + Send + Sync + 'static,
{
    type Settings = ResearchOverlaySettings<L>;
    type LeaderSelection = L;
    type CommitteeMembership = M;

    fn new(settings: Self::Settings) -> Self {
        let mut c = Committee::new();
        c.fill(settings.num_nodes, 0, None);
        Self {
            nodes: c.iter().copied().collect(),
            committee: c,
            l: settings.leader,
            leader_threshold: settings.leader_threshold,
            _committee_membership: Default::default(),
        }
    }

    fn rebuild(&mut self, _timeout_qc: TimeoutQc) {}

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

    fn parent_committee(&self, _id: NodeId) -> Option<Committee> {
        Some(std::iter::once(self.next_leader()).collect())
    }

    fn node_committee(&self, _id: NodeId) -> Committee {
        self.nodes.clone().into_iter().collect()
    }

    fn child_committees(&self, _id: NodeId) -> Vec<Committee> {
        vec![]
    }

    fn leaf_committees(&self, _id: NodeId) -> Vec<Committee> {
        vec![self.root_committee()]
    }

    fn next_leader(&self) -> NodeId {
        self.l.next_leader(&self.nodes)
    }

    fn super_majority_threshold(&self, _id: NodeId) -> usize {
        0
    }

    fn leader_super_majority_threshold(&self, _id: NodeId) -> usize {
        self.leader_threshold
    }

    fn update_leader_selection<F, E>(&self, f: F) -> Result<Self, E>
    where
        F: FnOnce(Self::LeaderSelection) -> Result<Self::LeaderSelection, E>,
    {
        match f(self.l.clone()) {
            Ok(leader_selection) => Ok(Self {
                l: leader_selection,
                ..self.clone()
            }),
            Err(e) => Err(e),
        }
    }

    fn root_committee(&self) -> Committee {
        self.committee.clone()
    }

    fn update_committees<F, E>(&self, f: F) -> Result<Self, E>
    where
        F: FnOnce(Self::CommitteeMembership) -> Result<Self::CommitteeMembership, E>,
    {
        todo!()
    }
}
