use super::{Committee, LeaderSelection, NodeId, Overlay};
use blake2::{digest::typenum::U32, Blake2b, Digest};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use std::collections::{HashMap, HashSet};

fn blake2b_hash(committee: &Committee) -> [u8; 32] {
    let mut hasher = Blake2b::<U32>::new();
    for member in committee {
        hasher.update(member);
    }
    hasher.finalize().into()
}

#[derive(Debug, Clone)]
struct Tree {
    inner_committees: Vec<NodeId>,
    membership_committees: HashMap<usize, Committee>,
    committee_id_to_index: HashMap<NodeId, usize>,
    committees_by_member: HashMap<NodeId, usize>,
}

impl Tree {
    pub fn new(nodes: &[NodeId], number_of_committees: usize) -> Self {
        assert!(number_of_committees > 0);

        let (inner_committees, membership_committees) =
            Self::build_committee_from_nodes_with_size(nodes, number_of_committees);

        let mut committee_id_to_index = HashMap::new();
        for (i, c) in inner_committees.iter().enumerate() {
            committee_id_to_index.insert(*c, i);
        }

        let mut committees_by_member = HashMap::new();
        for (committee, members) in &membership_committees {
            for member in members {
                committees_by_member.insert(*member, *committee);
            }
        }

        Self {
            inner_committees,
            membership_committees,
            committee_id_to_index,
            committees_by_member,
        }
    }

    fn build_committee_from_nodes_with_size(
        nodes: &[NodeId],
        number_of_committees: usize,
    ) -> (Vec<NodeId>, HashMap<usize, Committee>) {
        let committee_size = nodes.len() / number_of_committees;
        let remainder = nodes.len() % number_of_committees;

        let mut committees: Vec<Committee> = Vec::new();

        for n in 0..number_of_committees {
            let mut committee = HashSet::new();
            for node in &nodes[n * committee_size..(n + 1) * committee_size] {
                committee.insert(*node);
            }
            committees.push(committee);
        }

        if remainder > 0 {
            for node in &nodes[nodes.len() - remainder..] {
                committees.last_mut().unwrap().insert(*node);
            }
        }

        let hashes = committees.iter().map(blake2b_hash).collect::<Vec<_>>();
        (hashes, committees.into_iter().enumerate().collect())
    }

    #[cfg(test)]
    fn parent_committee(&self, committee_id: &NodeId) -> Option<&NodeId> {
        if committee_id == &self.inner_committees[0] {
            None
        } else {
            self.committee_id_to_index
                .get(committee_id)
                .map(|&idx| &self.inner_committees[(idx / 2).saturating_sub(1)])
        }
    }

    fn child_committees(&self, committee_id: &NodeId) -> (Option<&NodeId>, Option<&NodeId>) {
        let base = self
            .committee_id_to_index
            .get(committee_id)
            .map(|&idx| idx * 2)
            .unwrap_or(0);
        let first_child = base + 1;
        let second_child = base + 2;
        (
            self.inner_committees.get(first_child),
            self.inner_committees.get(second_child),
        )
    }

    fn leaf_committees(&self) -> HashMap<&NodeId, &Committee> {
        let total_leafs = (self.inner_committees.len() + 1) / 2;
        let mut leaf_committees = HashMap::new();
        for i in (self.inner_committees.len() - total_leafs)..self.inner_committees.len() {
            leaf_committees.insert(&self.inner_committees[i], &self.membership_committees[&i]);
        }
        leaf_committees
    }

    fn root_committee(&self) -> &Committee {
        &self.membership_committees[&0]
    }

    fn committee_by_committee_idx(&self, committee_idx: usize) -> Option<&Committee> {
        self.membership_committees.get(&committee_idx)
    }

    fn committee_idx_by_member_id(&self, member_id: &NodeId) -> Option<usize> {
        self.committees_by_member.get(member_id).copied()
    }

    fn committee_id_by_member_id(&self, member_id: &NodeId) -> Option<&NodeId> {
        self.committees_by_member
            .get(member_id)
            .map(|&idx| &self.inner_committees[idx])
    }

    fn committee_by_member_id(&self, member_id: &NodeId) -> Option<&Committee> {
        self.committee_idx_by_member_id(member_id)
            .and_then(|idx| self.committee_by_committee_idx(idx))
    }

    fn committee_by_committee_id(&self, committee_id: &NodeId) -> Option<&Committee> {
        self.committee_id_to_index
            .get(committee_id)
            .and_then(|&idx| self.committee_by_committee_idx(idx))
    }
}

#[derive(Debug, Clone)]
pub struct TreeOverlaySettings<L: LeaderSelection> {
    nodes: Vec<NodeId>,
    current_leader: NodeId,
    entropy: [u8; 32],
    number_of_committees: usize,
    leader: L,
}

#[derive(Debug, Clone)]
pub struct TreeOverlay<L> {
    entropy: [u8; 32],
    number_of_committees: usize,
    nodes: Vec<NodeId>,
    current_leader: NodeId,
    carnot_tree: Tree,
    leader: L,
}

impl<L> Overlay for TreeOverlay<L>
where
    L: LeaderSelection + Send + Sync + 'static,
{
    type Settings = TreeOverlaySettings<L>;

    type LeaderSelection = L;

    fn new(settings: Self::Settings) -> Self {
        let TreeOverlaySettings {
            mut nodes,
            current_leader,
            entropy,
            number_of_committees,
            leader,
        } = settings;
        let mut rng = StdRng::from_seed(entropy);
        nodes.shuffle(&mut rng);

        let carnot_tree = Tree::new(&nodes, number_of_committees);

        Self {
            entropy,
            number_of_committees,
            nodes,
            current_leader,
            carnot_tree,
            leader,
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
        parent.map_or(false, |p| child_parent.eq(p))
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
        self.parent_committee(id) == self.root_committee()
    }

    fn parent_committee(&self, id: NodeId) -> Committee {
        self.carnot_tree
            .committee_id_by_member_id(&id)
            .and_then(|parent_id| {
                self.carnot_tree.committee_by_committee_idx(
                    *self.carnot_tree.committee_id_to_index.get(parent_id)?,
                )
            })
            .cloned()
            .unwrap_or(self.root_committee())
    }

    fn child_committees(&self, id: NodeId) -> Vec<Committee> {
        match self.carnot_tree.child_committees(&id) {
            (None, None) => vec![],
            (None, Some(c)) | (Some(c), None) => vec![std::iter::once(*c).collect()],
            (Some(c1), Some(c2)) => vec![
                std::iter::once(*c1).collect(),
                std::iter::once(*c2).collect(),
            ],
        }
    }

    fn leaf_committees(&self, _id: NodeId) -> Vec<Committee> {
        self.carnot_tree
            .leaf_committees()
            .into_values()
            .cloned()
            .collect()
    }

    fn node_committee(&self, _id: NodeId) -> Committee {
        self.nodes.clone().into_iter().collect()
    }

    fn next_leader(&self) -> NodeId {
        let mut rng = StdRng::from_seed(self.entropy);
        *self.nodes.choose(&mut rng).unwrap()
    }

    fn super_majority_threshold(&self, id: NodeId) -> usize {
        if self.is_member_of_leaf_committee(id) {
            return 0;
        }
        self.carnot_tree
            .committee_by_member_id(&id)
            .map_or(0, |c| (c.len() * 2 / 3) + 1)
    }

    fn leader_super_majority_threshold(&self, _id: NodeId) -> usize {
        let root_committee = &self.carnot_tree.inner_committees[0];
        let children = self.carnot_tree.child_committees(root_committee);
        let children_size = children.0.map_or(0, |c| {
            self.carnot_tree
                .committee_by_committee_id(c)
                .map_or(0, |c| c.len())
        }) + children.1.map_or(0, |c| {
            self.carnot_tree
                .committee_by_committee_id(c)
                .map_or(0, |c| c.len())
        });
        let root_size = self.root_committee().len();
        ((root_size + children_size) as f64 * 0.67).ceil() as usize
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

impl<L> TreeOverlay<L>
where
    L: LeaderSelection + Send + Sync + 'static,
{
    pub fn advance(&self, entropy: [u8; 32], leader: L) -> Self {
        Self::new(TreeOverlaySettings {
            nodes: self.nodes.clone(),
            current_leader: self.next_leader(),
            entropy,
            number_of_committees: self.number_of_committees,
            leader,
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
    use crate::overlay::RoundRobin;

    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_carnot_tree_parenting() {
        let nodes: Vec<[u8; 32]> = (0..10).map(|i| [i as u8; 32]).collect();
        let tree = Tree::new(&nodes, 3);

        let root = &tree.inner_committees[0];
        let one = &tree.inner_committees[1];
        let two = &tree.inner_committees[2];

        assert_eq!(tree.parent_committee(one), Some(root));
        assert_eq!(tree.parent_committee(two), Some(root));
    }

    #[test]
    fn test_carnot_tree_root_parenting() {
        let nodes: Vec<[u8; 32]> = (0..10).map(|i| [i as u8; 32]).collect();
        let tree = Tree::new(&nodes, 3);

        let root = &tree.inner_committees[0];

        assert!(tree.parent_committee(root).is_none());
    }

    #[test]
    fn test_carnot_tree_childs() {
        let nodes: Vec<[u8; 32]> = (0..10).map(|i| [i as u8; 32]).collect();
        let tree = Tree::new(&nodes, 3);

        let root = &tree.inner_committees[0];
        let one = &tree.inner_committees[1];
        let two = &tree.inner_committees[2];

        assert_eq!(tree.child_committees(root), (Some(one), Some(two)));
    }

    #[test]
    fn test_carnot_overlay_leader() {
        let nodes: Vec<[u8; 32]> = (0..10).map(|i| [i as u8; 32]).collect();
        let overlay = TreeOverlay::new(TreeOverlaySettings {
            nodes: nodes.clone(),
            current_leader: nodes[0],
            entropy: [0; 32],
            number_of_committees: 3,
            leader: RoundRobin::new(),
        });

        assert_eq!(*overlay.leader(), nodes[0]);
    }

    #[test]
    fn test_next_leader_is_advance_current_leader() {
        let nodes: Vec<[u8; 32]> = (0..10).map(|i| [i as u8; 32]).collect();
        let mut overlay = TreeOverlay::new(TreeOverlaySettings {
            nodes: nodes.clone(),
            current_leader: nodes[0],
            entropy: [0; 32],
            number_of_committees: 3,
            leader: RoundRobin::new(),
        });

        let leader = overlay.next_leader();
        overlay = overlay.advance([1; 32], RoundRobin::new());

        assert_eq!(leader, *overlay.leader());
    }

    #[test]
    fn test_root_committee() {
        let nodes: Vec<[u8; 32]> = (0..10).map(|i| [i as u8; 32]).collect();
        let overlay = TreeOverlay::new(TreeOverlaySettings {
            nodes: nodes.clone(),
            current_leader: nodes[0],
            entropy: [0; 32],
            number_of_committees: 3,
            leader: RoundRobin::new(),
        });

        let mut expected_root = HashSet::new();
        expected_root.insert(nodes[9]);
        expected_root.extend(nodes[0..3].iter());

        assert_eq!(overlay.root_committee(), expected_root);
    }

    #[test]
    fn test_leaf_committees() {
        let nodes: Vec<[u8; 32]> = (0..10).map(|i| [i as u8; 32]).collect();
        let overlay = TreeOverlay::new(TreeOverlaySettings {
            nodes: nodes.clone(),
            current_leader: nodes[0],
            entropy: [0; 32],
            number_of_committees: 3,
            leader: RoundRobin::new(),
        });

        assert_eq!(
            overlay.leaf_committees([0; 32]),
            vec![
                nodes[3..6].iter().cloned().collect(),
                nodes[6..9].iter().cloned().collect()
            ]
        );
    }

    #[test]
    fn test_super_majority_threshold_for_leaf() {
        let nodes: Vec<[u8; 32]> = (0..10).map(|i| [i as u8; 32]).collect();
        let overlay = TreeOverlay::new(TreeOverlaySettings {
            nodes: nodes.clone(),
            current_leader: nodes[0],
            entropy: [0; 32],
            number_of_committees: 3,
            leader: RoundRobin::new(),
        });

        assert_eq!(overlay.super_majority_threshold(nodes[8]), 0);
    }

    #[test]
    fn test_super_majority_threshold_for_root_member() {
        let nodes: Vec<[u8; 32]> = (0..10).map(|i| [i as u8; 32]).collect();
        let overlay = TreeOverlay::new(TreeOverlaySettings {
            nodes: nodes.clone(),
            current_leader: nodes[0],
            entropy: [0; 32],
            number_of_committees: 3,
            leader: RoundRobin::new(),
        });

        assert_eq!(overlay.super_majority_threshold(nodes[0]), 3);
    }

    #[test]
    fn test_leader_super_majority_threshold() {
        let nodes: Vec<[u8; 32]> = (0..10).map(|i| [i as u8; 32]).collect();
        let overlay = TreeOverlay::new(TreeOverlaySettings {
            nodes: nodes.clone(),
            current_leader: nodes[0],
            entropy: [0; 32],
            number_of_committees: 3,
            leader: RoundRobin::new(),
        });

        assert_eq!(overlay.leader_super_majority_threshold([0; 32]), 7);
    }
}
