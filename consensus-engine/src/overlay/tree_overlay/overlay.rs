use super::tree::Tree;
use crate::overlay::CommitteeMembership;
use crate::{overlay::LeaderSelection, Committee, NodeId, Overlay};

#[derive(Debug, Clone)]
pub struct TreeOverlaySettings<L: LeaderSelection, M: CommitteeMembership> {
    pub nodes: Vec<NodeId>,
    pub current_leader: NodeId,
    pub number_of_committees: usize,
    pub leader: L,
    pub committee_membership: M,
}

#[derive(Debug, Clone)]
pub struct TreeOverlay<L, M> {
    pub(super) number_of_committees: usize,
    pub(super) nodes: Vec<NodeId>,
    pub(super) current_leader: NodeId,
    pub(super) carnot_tree: Tree,
    pub(super) leader: L,
    pub(super) committee_membership: M,
}

impl<L, M> Overlay for TreeOverlay<L, M>
where
    L: LeaderSelection + Send + Sync + 'static,
    M: CommitteeMembership + Send + Sync + 'static,
{
    type Settings = TreeOverlaySettings<L, M>;

    type LeaderSelection = L;
    type CommitteeMembership = M;

    fn new(settings: Self::Settings) -> Self {
        let TreeOverlaySettings {
            nodes,
            current_leader,
            number_of_committees,
            leader,
            committee_membership,
        } = settings;

        let nodes = committee_membership.reshape_committees(&nodes);
        let carnot_tree = Tree::new(&nodes, number_of_committees);

        Self {
            number_of_committees,
            nodes,
            current_leader,
            carnot_tree,
            leader,
            committee_membership,
        }
    }

    fn root_committee(&self) -> Committee {
        self.carnot_tree.root_committee().clone()
    }

    fn rebuild(&mut self, _timeout_qc: crate::TimeoutQc) {
        unimplemented!("do nothing for now")
    }

    fn is_member_of_child_committee(&self, parent: NodeId, child: NodeId) -> bool {
        let child_parent = self.parent_committee(child);
        let parent = self.carnot_tree.committee_by_member_id(&parent);
        child_parent.as_ref() == parent
    }

    fn is_member_of_root_committee(&self, id: NodeId) -> bool {
        self.carnot_tree.root_committee().contains(&id)
    }

    fn is_member_of_leaf_committee(&self, id: NodeId) -> bool {
        self.carnot_tree
            .leaf_committees()
            .values()
            .any(|committee| committee.contains(&id))
    }

    fn is_child_of_root_committee(&self, id: NodeId) -> bool {
        self.parent_committee(id)
            .map(|c| c == self.root_committee())
            .unwrap_or(false)
    }

    fn parent_committee(&self, id: NodeId) -> Option<Committee> {
        self.carnot_tree.parent_committee_from_member_id(&id)
    }

    fn child_committees(&self, id: NodeId) -> Vec<Committee> {
        // Lookup committee index by member id, then committee id by index.
        self.carnot_tree
            .committees_by_member
            .get(&id)
            .and_then(|committee_idx| self.carnot_tree.inner_committees.get(*committee_idx))
            .map(|committee_id| {
                let (l, r) = self.carnot_tree.child_committees(committee_id);
                let extract_committee = |committee_id| {
                    self.carnot_tree
                        .committee_id_to_index
                        .get(committee_id)
                        .and_then(|committee_idx| {
                            self.carnot_tree.membership_committees.get(committee_idx)
                        })
                };
                let l = l.and_then(extract_committee).into_iter().cloned();
                let r = r.and_then(extract_committee).into_iter().cloned();
                l.chain(r).collect()
            })
            .expect("NodeId not found in overlay")
    }

    fn leaf_committees(&self, _id: NodeId) -> Vec<Committee> {
        self.carnot_tree
            .leaf_committees()
            .into_values()
            .cloned()
            .collect()
    }

    fn node_committee(&self, id: NodeId) -> Committee {
        self.carnot_tree
            .committees_by_member
            .get(&id)
            .and_then(|committee_index| self.carnot_tree.membership_committees.get(committee_index))
            .cloned()
            .unwrap_or_default()
    }

    fn next_leader(&self) -> NodeId {
        self.leader.next_leader(&self.nodes)
    }

    fn super_majority_threshold(&self, id: NodeId) -> usize {
        if self.is_member_of_leaf_committee(id) {
            return 0;
        }
        self.carnot_tree
            .committee_by_member_id(&id)
            .map(|c| (c.len() * 2 / 3) + 1)
            .expect("node is not part of any committee")
    }

    // TODO: Carnot node in sim does not send votes to the next leader from the child committee of
    // root committee yet. *For now* leader super majority threshold should be calculated only from
    // the number of root committee nodes. The code will be reverted once vote sending from
    // child committee of root committee is added to Carnot node.
    fn leader_super_majority_threshold(&self, _id: NodeId) -> usize {
        // let root_committee = &self.carnot_tree.inner_committees[0];
        // let children = self.carnot_tree.child_committees(root_committee);
        // let children_size = children.0.map_or(0, |c| {
        //     self.carnot_tree
        //         .committee_by_committee_id(c)
        //         .map_or(0, |c| c.len())
        // }) + children.1.map_or(0, |c| {
        //     self.carnot_tree
        //         .committee_by_committee_id(c)
        //         .map_or(0, |c| c.len())
        // });
        // let root_size = self.root_committee().len();
        // let committee_size = root_size + children_size;
        let committee_size = self.root_committee().len();
        (committee_size * 2 / 3) + 1
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
            let settings = TreeOverlaySettings {
                nodes: self.nodes.clone(),
                current_leader: self.current_leader,
                number_of_committees: self.number_of_committees,
                leader: self.leader.clone(),
                committee_membership,
            };
            Self::new(settings)
        })
    }
}

impl<L, M> TreeOverlay<L, M>
where
    L: LeaderSelection + Send + Sync + 'static,
    M: CommitteeMembership + Send + Sync + 'static,
{
    pub fn advance(&self, leader: L, committee_membership: M) -> Self {
        Self::new(TreeOverlaySettings {
            nodes: self.nodes.clone(),
            current_leader: self.next_leader(),
            number_of_committees: self.number_of_committees,
            leader,
            committee_membership,
        })
    }

    pub fn is_leader(&self, id: &NodeId) -> bool {
        id == &self.current_leader
    }

    pub fn leader(&self) -> &NodeId {
        &self.current_leader
    }
}

#[cfg(test)]
mod tests {
    use crate::overlay::leadership::RoundRobin;
    use crate::overlay::membership::FisherYatesShuffle;
    use crate::Overlay;

    use super::*;
    const ENTROPY: [u8; 32] = [0; 32];

    #[test]
    fn test_carnot_overlay_leader() {
        let nodes: Vec<_> = (0..10).map(|i| NodeId::new([i as u8; 32])).collect();
        let overlay = TreeOverlay::new(TreeOverlaySettings {
            nodes: nodes.clone(),
            current_leader: nodes[0],
            number_of_committees: 3,
            leader: RoundRobin::new(),
            committee_membership: FisherYatesShuffle::new(ENTROPY),
        });

        assert_eq!(*overlay.leader(), nodes[0]);
    }

    #[test]
    fn test_next_leader_is_advance_current_leader() {
        let nodes: Vec<_> = (0..10).map(|i| NodeId::new([i as u8; 32])).collect();
        let mut overlay = TreeOverlay::new(TreeOverlaySettings {
            nodes: nodes.clone(),
            current_leader: nodes[0],
            number_of_committees: 3,
            leader: RoundRobin::new(),
            committee_membership: FisherYatesShuffle::new(ENTROPY),
        });

        let leader = overlay.next_leader();
        overlay = overlay.advance(RoundRobin::new(), FisherYatesShuffle::new(ENTROPY));

        assert_eq!(leader, *overlay.leader());
    }

    #[test]
    fn test_root_committee() {
        let nodes: Vec<_> = (0..10).map(|i| NodeId::new([i as u8; 32])).collect();
        let overlay = TreeOverlay::new(TreeOverlaySettings {
            current_leader: nodes[0],
            nodes,
            number_of_committees: 3,
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
        let overlay = TreeOverlay::new(TreeOverlaySettings {
            current_leader: nodes[0],
            nodes,
            number_of_committees: 3,
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
        let mut c1 = overlay.nodes[3..6].to_vec();
        c1.sort();
        let mut c2 = overlay.nodes[6..9].to_vec();
        c2.sort();
        let mut expected = vec![c1, c2];
        expected.sort();
        assert_eq!(leaf_committees, expected);
    }

    #[test]
    fn test_super_majority_threshold_for_leaf() {
        let nodes: Vec<_> = (0..10).map(|i| NodeId::new([i as u8; 32])).collect();
        let overlay = TreeOverlay::new(TreeOverlaySettings {
            current_leader: nodes[0],
            nodes,
            number_of_committees: 3,
            leader: RoundRobin::new(),
            committee_membership: FisherYatesShuffle::new(ENTROPY),
        });

        assert_eq!(overlay.super_majority_threshold(overlay.nodes[8]), 0);
    }

    #[test]
    fn test_super_majority_threshold_for_root_member() {
        let nodes: Vec<_> = (0..10).map(|i| NodeId::new([i as u8; 32])).collect();
        let overlay = TreeOverlay::new(TreeOverlaySettings {
            current_leader: nodes[0],
            nodes,
            number_of_committees: 3,
            leader: RoundRobin::new(),
            committee_membership: FisherYatesShuffle::new(ENTROPY),
        });

        assert_eq!(overlay.super_majority_threshold(overlay.nodes[0]), 3);
    }

    #[test]
    fn test_leader_super_majority_threshold() {
        let nodes: Vec<_> = (0..10).map(|i| NodeId::new([i as u8; 32])).collect();
        let overlay = TreeOverlay::new(TreeOverlaySettings {
            nodes: nodes.clone(),
            current_leader: nodes[0],
            number_of_committees: 3,
            leader: RoundRobin::new(),
            committee_membership: FisherYatesShuffle::new(ENTROPY),
        });

        assert_eq!(
            overlay.leader_super_majority_threshold(NodeId::new([0; 32])),
            3
        );
    }
}
