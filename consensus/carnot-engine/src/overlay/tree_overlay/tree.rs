use crate::{Committee, CommitteeId, NodeId};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub(super) struct Tree {
    pub(super) inner_committees: Vec<CommitteeId>,
    pub(super) membership_committees: HashMap<usize, Committee>,
    pub(super) committee_id_to_index: HashMap<CommitteeId, usize>,
    pub(super) committees_by_member: HashMap<NodeId, usize>,
}

impl Tree {
    pub fn new(nodes: &[NodeId], number_of_committees: usize) -> Self {
        assert!(number_of_committees > 0);

        let (inner_committees, membership_committees) =
            Self::build_committee_from_nodes_with_size(nodes, number_of_committees);

        let committee_id_to_index = inner_committees
            .iter()
            .copied()
            .enumerate()
            .map(|(idx, c)| (c, idx))
            .collect();

        let committees_by_member = membership_committees
            .iter()
            .flat_map(|(committee, members)| members.iter().map(|member| (*member, *committee)))
            .collect();

        Self {
            inner_committees,
            membership_committees,
            committee_id_to_index,
            committees_by_member,
        }
    }

    pub(super) fn build_committee_from_nodes_with_size(
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

    pub(super) fn parent_committee(&self, committee_id: &CommitteeId) -> Option<&CommitteeId> {
        if committee_id == &self.inner_committees[0] {
            None
        } else {
            self.committee_id_to_index
                .get(committee_id)
                .map(|&idx| &self.inner_committees[(idx - 1) / 2])
        }
    }

    pub(super) fn parent_committee_from_member_id(&self, id: &NodeId) -> Option<Committee> {
        let committee_id = self.committee_id_by_member_id(id)?;
        let parent_id = self.parent_committee(committee_id)?;
        self.committee_by_committee_idx(self.committee_id_to_index[parent_id])
            .cloned()
    }

    pub(super) fn child_committees(
        &self,
        committee_id: &CommitteeId,
    ) -> (Option<&CommitteeId>, Option<&CommitteeId>) {
        let Some(base) = self
            .committee_id_to_index
            .get(committee_id)
            .map(|&idx| idx * 2)
        else {
            return (None, None);
        };
        let first_child = base + 1;
        let second_child = base + 2;
        (
            self.inner_committees.get(first_child),
            self.inner_committees.get(second_child),
        )
    }

    pub(super) fn leaf_committees(&self) -> HashMap<&CommitteeId, &Committee> {
        let total_leafs = (self.inner_committees.len() + 1) / 2;
        let mut leaf_committees = HashMap::new();
        for i in (self.inner_committees.len() - total_leafs)..self.inner_committees.len() {
            leaf_committees.insert(&self.inner_committees[i], &self.membership_committees[&i]);
        }
        leaf_committees
    }

    pub(super) fn root_committee(&self) -> &Committee {
        &self.membership_committees[&0]
    }

    pub(super) fn committee_by_committee_idx(&self, committee_idx: usize) -> Option<&Committee> {
        self.membership_committees.get(&committee_idx)
    }

    pub(super) fn committee_idx_by_member_id(&self, member_id: &NodeId) -> Option<usize> {
        self.committees_by_member.get(member_id).copied()
    }

    pub(super) fn committee_id_by_member_id(&self, member_id: &NodeId) -> Option<&CommitteeId> {
        self.committees_by_member
            .get(member_id)
            .map(|&idx| &self.inner_committees[idx])
    }

    pub(super) fn committee_by_member_id(&self, member_id: &NodeId) -> Option<&Committee> {
        self.committee_idx_by_member_id(member_id)
            .and_then(|idx| self.committee_by_committee_idx(idx))
    }

    #[allow(dead_code)]
    pub(super) fn committee_by_committee_id(
        &self,
        committee_id: &CommitteeId,
    ) -> Option<&Committee> {
        self.committee_id_to_index
            .get(committee_id)
            .and_then(|&idx| self.committee_by_committee_idx(idx))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashSet, VecDeque};

    use super::*;

    #[test]
    fn test_carnot_tree_parenting() {
        let nodes: Vec<_> = (0..10).map(|i| NodeId::new([i as u8; 32])).collect();
        let tree = Tree::new(&nodes, 3);

        let root = &tree.inner_committees[0];
        let one = &tree.inner_committees[1];
        let two = &tree.inner_committees[2];

        assert_eq!(tree.parent_committee(one), Some(root));
        assert_eq!(tree.parent_committee(two), Some(root));
    }

    #[test]
    fn test_carnot_tree_root_parenting() {
        let nodes: Vec<_> = (0..10).map(|i| NodeId::new([i as u8; 32])).collect();
        let tree = Tree::new(&nodes, 3);

        let root = &tree.inner_committees[0];

        assert!(tree.parent_committee(root).is_none());
    }

    #[test]
    fn test_carnot_tree_childs() {
        let nodes: Vec<_> = (0..10).map(|i| NodeId::new([i as u8; 32])).collect();
        let tree = Tree::new(&nodes, 3);

        let root = &tree.inner_committees[0];
        let one = &tree.inner_committees[1];
        let two = &tree.inner_committees[2];

        assert_eq!(tree.child_committees(root), (Some(one), Some(two)));
    }

    #[test]
    fn test_carnot_tree_leaf_committees() {
        let test_cases = [
            (3, 2),
            (4, 2),
            (5, 3),
            (6, 3),
            (7, 4),
            (8, 4),
            (9, 5),
            (10, 5),
            (11, 6),
            (12, 6),
            (13, 7),
            (14, 7),
            (15, 8),
            (17, 9),
            (18, 9),
            (31, 16),
        ];
        for (committees, expected_leaves) in test_cases {
            let nodes: Vec<_> = (0..committees)
                .map(|i| NodeId::new([i as u8; 32]))
                .collect();
            let tree = Tree::new(&nodes, committees);

            let leaves = tree.leaf_committees();

            assert_eq!(leaves.len(), expected_leaves);
        }
    }

    #[test]
    fn test_carnot_tree_leaf_parents() {
        let nodes: Vec<_> = (0..10).map(|i| NodeId::new([i as u8; 32])).collect();
        let tree = Tree::new(&nodes, 7);

        // Helper function to get nodes from a committee.
        fn get_nodes_from_committee(tree: &Tree, committee: &CommitteeId) -> Vec<NodeId> {
            tree.committee_by_committee_id(committee)
                .unwrap()
                .iter()
                .cloned()
                .collect()
        }

        // Helper function to test committee and parent relationship.
        fn test_committee_parent(tree: &Tree, child: &CommitteeId, parent: &CommitteeId) {
            let child_nodes = get_nodes_from_committee(tree, child);
            let node_comm = tree.committee_by_member_id(&child_nodes[0]).unwrap();
            assert_eq!(
                node_comm.id::<blake2::Blake2b<digest::typenum::U32>>(),
                *child
            );
            assert_eq!(
                tree.parent_committee_from_member_id(&child_nodes[0])
                    .map(|c| c.id::<blake2::Blake2b<digest::typenum::U32>>()),
                Some(*parent)
            );
        }

        // (Upper Committee, (Leaf 1, Leaf 2))
        let test_cases = [
            (
                &tree.inner_committees[1],
                (
                    Some(&tree.inner_committees[3]),
                    Some(&tree.inner_committees[4]),
                ),
            ),
            (
                &tree.inner_committees[2],
                (
                    Some(&tree.inner_committees[5]),
                    Some(&tree.inner_committees[6]),
                ),
            ),
        ];
        test_cases.iter().for_each(|tcase| {
            let child_comm = tree.child_committees(tcase.0);
            assert_eq!(child_comm, tcase.1);
            test_committee_parent(&tree, tcase.1 .0.unwrap(), tcase.0);
            test_committee_parent(&tree, tcase.1 .1.unwrap(), tcase.0);
        });
    }

    fn carnot_tree_level_sizes_from_top(tree: Tree) -> HashMap<i32, i32> {
        let root = tree.root_committee();
        let root_id = root.id::<blake2::Blake2b<digest::typenum::U32>>();

        let mut queue = VecDeque::new();
        queue.push_back((root_id, 0));

        let mut level_sizes = HashMap::new();

        while let Some((current_id, level)) = queue.pop_front() {
            *level_sizes.entry(level).or_insert(0) += 1;

            let children = tree.child_committees(&current_id);
            if let Some(id) = children.0 {
                queue.push_back((*id, level + 1));
            }
            if let Some(id) = children.1 {
                queue.push_back((*id, level + 1));
            }
        }

        level_sizes
    }

    fn next_pow2(n: usize) -> usize {
        let mut n = n - 1;
        n |= n >> 1;
        n |= n >> 2;
        n |= n >> 4;
        n |= n >> 8;
        n |= n >> 16;
        n |= n >> 32;
        n + 1
    }

    fn is_full_balanced_binary_tree(committee_count: usize) -> bool {
        let mut count = 1;
        while count < committee_count + 1 {
            count *= 2;
        }
        count == committee_count + 1
    }

    fn carnot_tree_level_sizes_from_bottom(tree: Tree) -> HashMap<i32, i32> {
        // TODO: Find a more elegant way to visit parents of leaves from different levels.
        let leaves: HashMap<CommitteeId, i32> =
            if is_full_balanced_binary_tree(tree.inner_committees.len()) {
                tree.leaf_committees().keys().map(|c| (**c, 0)).collect()
            } else {
                let next_full_tree_committees = next_pow2(tree.inner_committees.len()) - 1;
                let next_full_tree_leafs = (next_full_tree_committees + 1) / 2;
                let prev_full_tree_committees = next_full_tree_committees - next_full_tree_leafs;

                let mut leaf_committees: HashMap<CommitteeId, i32> =
                    tree.leaf_committees().keys().map(|c| (**c, 1)).collect();

                for i in prev_full_tree_committees..tree.inner_committees.len() {
                    leaf_committees.insert(tree.inner_committees[i], 0);
                }

                leaf_committees
            };

        let mut queue = VecDeque::new();
        let mut level_sizes = HashMap::new();
        let mut visited = HashSet::new();

        for (leaf_id, level) in leaves {
            queue.push_back((leaf_id, level));
            visited.insert(leaf_id);
        }

        while let Some((current_id, level)) = queue.pop_front() {
            *level_sizes.entry(level).or_insert(0) += 1;

            if let Some(parent_id) = tree.parent_committee(&current_id) {
                if !visited.contains(parent_id) {
                    queue.push_back((*parent_id, level + 1));
                    visited.insert(*parent_id);
                }
            }
        }

        level_sizes
    }

    #[test]
    fn test_carnot_tree_level_sizes() {
        struct TestCase {
            num_committees: usize,
            max_depth: usize,
            expected_level_sizes: HashMap<i32, i32>,
        }

        let test_cases = vec![
            TestCase {
                num_committees: 1,
                max_depth: 1,
                expected_level_sizes: [(0, 1)].iter().cloned().collect(),
            },
            TestCase {
                num_committees: 3,
                max_depth: 2,
                expected_level_sizes: [(0, 1), (1, 2)].iter().cloned().collect(),
            },
            TestCase {
                num_committees: 7,
                max_depth: 3,
                expected_level_sizes: [(0, 1), (1, 2), (2, 4)].iter().cloned().collect(),
            },
            TestCase {
                num_committees: 15,
                max_depth: 4,
                expected_level_sizes: [(0, 1), (1, 2), (2, 4), (3, 8)].iter().cloned().collect(),
            },
            TestCase {
                num_committees: 11,
                max_depth: 4,
                expected_level_sizes: [(0, 1), (1, 2), (2, 4), (3, 4)].iter().cloned().collect(),
            },
            TestCase {
                num_committees: 25,
                max_depth: 5,
                expected_level_sizes: [(0, 1), (1, 2), (2, 4), (3, 8), (4, 10)]
                    .iter()
                    .cloned()
                    .collect(),
            },
            TestCase {
                num_committees: 40,
                max_depth: 6,
                expected_level_sizes: [(0, 1), (1, 2), (2, 4), (3, 8), (4, 16), (5, 9)]
                    .iter()
                    .cloned()
                    .collect(),
            },
        ];

        for case in test_cases {
            let nodes: Vec<_> = (0..case.num_committees)
                .map(|i| NodeId::new([i as u8; 32]))
                .collect();

            let tree = Tree::new(&nodes, case.num_committees);

            let level_sizes_top = carnot_tree_level_sizes_from_top(tree.clone());
            let level_sizes_bottom = carnot_tree_level_sizes_from_bottom(tree);

            for (top_level, top_size) in level_sizes_top.iter() {
                let bottom_level = case.max_depth as i32 - top_level - 1;
                let bottom_size = level_sizes_bottom.get(&bottom_level).unwrap();
                assert_eq!(top_size, bottom_size);
            }

            assert_eq!(level_sizes_top, case.expected_level_sizes);
        }
    }
}
