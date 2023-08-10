use super::LeaderSelection;
use crate::overlay::CommitteeMembership;
use crate::{Committee, CommitteeId, NodeId, Overlay};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct BranchOverlaySettings<L: LeaderSelection, M: CommitteeMembership> {
    pub nodes: Vec<NodeId>,
    pub current_leader: NodeId,
    pub number_of_levels: usize,
    pub leader: L,
    pub committee_membership: M,
}

#[derive(Clone, Debug, PartialEq)]
/// Branch overlay with a single committee and round robin leader selection.
pub struct BranchOverlay<L: LeaderSelection, M: CommitteeMembership> {
    nodes: Vec<NodeId>,
    leader: L,
    committee_membership: M,
    current_leader: NodeId,
    number_of_committees: usize,
    membership_committees: HashMap<usize, Committee>,
    committees_by_member: HashMap<NodeId, usize>,
}

impl<L, M> Overlay for BranchOverlay<L, M>
where
    L: LeaderSelection + Send + Sync + 'static,
    M: CommitteeMembership + Send + Sync + 'static,
{
    type Settings = BranchOverlaySettings<L, M>;
    type LeaderSelection = L;
    type CommitteeMembership = M;

    fn new(settings: Self::Settings) -> Self {
        let BranchOverlaySettings {
            nodes,
            current_leader,
            number_of_levels,
            leader,
            committee_membership,
        } = settings;
        let (inner_committees, membership_committees) =
            build_committee_from_nodes_with_size(&nodes, number_of_levels);

        assert!(number_of_levels == inner_committees.len());

        let committees_by_member = membership_committees
            .iter()
            .flat_map(|(committee, members)| members.iter().map(|member| (*member, *committee)))
            .collect();
        Self {
            number_of_committees: number_of_levels,
            nodes,
            current_leader,
            leader,
            committee_membership,
            membership_committees,
            committees_by_member,
        }
    }

    fn root_committee(&self) -> Committee {
        self.membership_committees[&0].clone()
    }

    fn rebuild(&mut self, _timeout_qc: crate::TimeoutQc) {
        // do nothing for now
    }

    fn is_member_of_child_committee(&self, parent: NodeId, child: NodeId) -> bool {
        self.committees_by_member[&child]
            .checked_sub(1)
            .map(|child_parent| {
                let parent = self.committees_by_member[&parent];
                child_parent == parent
            })
            .unwrap_or(false)
    }

    fn is_member_of_root_committee(&self, id: NodeId) -> bool {
        self.root_committee().contains(&id)
    }

    fn is_member_of_leaf_committee(&self, id: NodeId) -> bool {
        self.membership_committees[&(self.number_of_committees - 1)].contains(&id)
    }

    fn is_child_of_root_committee(&self, id: NodeId) -> bool {
        self.number_of_committees > 1 && self.membership_committees[&1].contains(&id)
    }

    fn parent_committee(&self, id: NodeId) -> Option<Committee> {
        self.committees_by_member[&id]
            .checked_sub(1)
            .map(|parent_id| self.membership_committees[&parent_id].clone())
    }

    fn child_committees(&self, id: NodeId) -> Vec<Committee> {
        if self.is_member_of_leaf_committee(id) {
            vec![]
        } else {
            let parent = self.committees_by_member[&id] + 1;
            vec![self.membership_committees[&parent].clone()]
        }
    }

    fn leaf_committees(&self, _id: NodeId) -> Vec<Committee> {
        vec![self.membership_committees[&(self.number_of_committees - 1)].clone()]
    }

    fn node_committee(&self, id: NodeId) -> Committee {
        let committee_id = self.committees_by_member[&id];
        self.membership_committees[&committee_id].clone()
    }

    fn next_leader(&self) -> NodeId {
        self.leader.next_leader(&self.nodes)
    }

    fn super_majority_threshold(&self, id: NodeId) -> usize {
        if self.is_member_of_leaf_committee(id) {
            return 0;
        }
        let committee_size = self.root_committee().len();
        (committee_size * 2 / 3) + 1
    }

    fn leader_super_majority_threshold(&self, id: NodeId) -> usize {
        self.super_majority_threshold(id)
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

    fn update_committees<F, E>(&self, f: F) -> Result<Self, E>
    where
        F: FnOnce(Self::CommitteeMembership) -> Result<Self::CommitteeMembership, E>,
    {
        f(self.committee_membership.clone()).map(|committee_membership| {
            let settings = BranchOverlaySettings {
                nodes: self.nodes.clone(),
                current_leader: self.current_leader,
                number_of_levels: self.number_of_committees,
                leader: self.leader.clone(),
                committee_membership,
            };
            Self::new(settings)
        })
    }
}

fn build_committee_from_nodes_with_size(
    nodes: &[NodeId],
    number_of_committees: usize,
) -> (Vec<CommitteeId>, HashMap<usize, Committee>) {
    let committee_size = nodes.len() / number_of_committees;
    let remainder = nodes.len() % number_of_committees;

    let mut committees: Vec<Committee> = (0..number_of_committees)
        .map(|n| {
            nodes[n * committee_size..(n + 1) * committee_size]
                .iter()
                .cloned()
                .collect()
        })
        .collect();

    // Refill committees with extra nodes
    if remainder != 0 {
        for i in 0..remainder {
            let node = nodes[nodes.len() - remainder + i];
            let committee_index = i % number_of_committees;
            committees[committee_index].insert(node);
        }
    }

    let hashes = committees
        .iter()
        .map(Committee::id::<blake2::Blake2b<digest::typenum::U32>>)
        .collect::<Vec<_>>();
    (hashes, committees.into_iter().enumerate().collect())
}

#[cfg(test)]
mod tests {
    use crate::overlay::{FisherYatesShuffle, RoundRobin};

    use super::*;
    const ENTROPY: [u8; 32] = [0; 32];

    #[test]
    fn test_root_committee() {
        let nodes: Vec<_> = (0..10).map(|i| NodeId::new([i as u8; 32])).collect();
        let overlay = BranchOverlay::new(BranchOverlaySettings {
            current_leader: nodes[0],
            nodes,
            number_of_levels: 3,
            leader: RoundRobin::new(),
            committee_membership: FisherYatesShuffle::new(ENTROPY),
        });

        let mut expected_root = Committee::new();
        expected_root.insert(overlay.nodes[9]);
        expected_root.extend(overlay.nodes[0..3].iter());

        assert_eq!(overlay.root_committee(), expected_root);
    }

    #[test]
    fn test_leaf_committees() {
        let nodes: Vec<_> = (0..10).map(|i| NodeId::new([i as u8; 32])).collect();
        let overlay = BranchOverlay::new(BranchOverlaySettings {
            current_leader: nodes[0],
            nodes,
            number_of_levels: 3,
            leader: RoundRobin::new(),
            committee_membership: FisherYatesShuffle::new(ENTROPY),
        });

        let mut leaf_committees = overlay
            .leaf_committees(NodeId::new([0; 32]))
            .into_iter()
            .map(|s| {
                let mut vec = s.into_iter().collect::<Vec<_>>();
                vec.sort();
                vec
            })
            .collect::<Vec<_>>();
        leaf_committees.sort();
        let mut c1 = overlay.nodes[6..9].to_vec();
        c1.sort();
        let mut expected = vec![c1];
        expected.sort();
        assert_eq!(leaf_committees, expected);
    }

    #[test]
    fn test_child_committees() {
        let nodes: Vec<_> = (0..40).map(|i| NodeId::new([i as u8; 32])).collect();
        let overlay = BranchOverlay::new(BranchOverlaySettings {
            current_leader: nodes[0],
            nodes,
            number_of_levels: 4,
            leader: RoundRobin::new(),
            committee_membership: FisherYatesShuffle::new(ENTROPY),
        });

        let mut child_committees_0 = overlay
            .child_committees(NodeId::new([0; 32]))
            .into_iter()
            .map(|s| {
                let mut vec = s.into_iter().collect::<Vec<_>>();
                vec.sort();
                vec
            })
            .collect::<Vec<_>>();

        let mut child_committees_1 = overlay
            .child_committees(NodeId::new([10; 32]))
            .into_iter()
            .map(|s| {
                let mut vec = s.into_iter().collect::<Vec<_>>();
                vec.sort();
                vec
            })
            .collect::<Vec<_>>();

        child_committees_0.sort();
        child_committees_1.sort();
        let mut c0 = overlay.nodes[10..20].to_vec();
        c0.sort();
        let mut c1 = overlay.nodes[20..30].to_vec();
        c1.sort();
        let mut expected_0 = vec![c0];
        expected_0.sort();
        let mut expected_1 = vec![c1];
        expected_1.sort();

        assert_eq!(child_committees_0, expected_0);
        assert_eq!(child_committees_1, expected_1);
    }
}
