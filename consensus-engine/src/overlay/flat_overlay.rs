use super::threshold::{apply_threshold, default_super_majority_threshold, deser_fraction};
use super::LeaderSelection;
use crate::overlay::CommitteeMembership;
use crate::{NodeId, Overlay};
use fraction::Fraction;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

#[derive(Clone, Debug, PartialEq)]
/// Flat overlay with a single committee and round robin leader selection.
pub struct FlatOverlay<L: LeaderSelection, M: CommitteeMembership> {
    nodes: Vec<NodeId>,
    leader: L,
    leader_threshold: Fraction,
    _committee_membership: PhantomData<M>,
}

impl<L, M> Overlay for FlatOverlay<L, M>
where
    L: LeaderSelection + Send + Sync + 'static,
    M: CommitteeMembership + Send + Sync + 'static,
{
    type Settings = FlatOverlaySettings<L>;
    type LeaderSelection = L;
    type CommitteeMembership = M;

    fn new(
        FlatOverlaySettings {
            leader,
            nodes,
            leader_super_majority_threshold,
        }: Self::Settings,
    ) -> Self {
        Self {
            nodes,
            leader,
            leader_threshold: leader_super_majority_threshold
                .unwrap_or_else(default_super_majority_threshold),
            _committee_membership: Default::default(),
        }
    }

    fn root_committee(&self) -> crate::Committee {
        self.nodes.clone().into_iter().collect()
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

    fn parent_committee(&self, _id: NodeId) -> Option<crate::Committee> {
        Some(std::iter::once(self.next_leader()).collect())
    }

    fn child_committees(&self, _id: NodeId) -> Vec<crate::Committee> {
        vec![]
    }

    fn leaf_committees(&self, _id: NodeId) -> Vec<crate::Committee> {
        vec![self.root_committee()]
    }

    fn node_committee(&self, _id: NodeId) -> crate::Committee {
        self.nodes.clone().into_iter().collect()
    }

    fn next_leader(&self) -> NodeId {
        self.leader.next_leader(&self.nodes)
    }

    fn super_majority_threshold(&self, _id: NodeId) -> usize {
        0
    }

    fn leader_super_majority_threshold(&self, _id: NodeId) -> usize {
        apply_threshold(self.nodes.len(), self.leader_threshold)
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

    fn update_committees<F, E>(&self, _f: F) -> Result<Self, E>
    where
        F: FnOnce(Self::CommitteeMembership) -> Result<Self::CommitteeMembership, E>,
    {
        Ok(self.clone())
    }
}

#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FlatOverlaySettings<L> {
    pub nodes: Vec<NodeId>,
    /// A fraction representing the threshold in the form `<num>/<den>'
    /// Defaults to 2/3
    #[serde(with = "deser_fraction")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leader_super_majority_threshold: Option<Fraction>,
    pub leader: L,
}
