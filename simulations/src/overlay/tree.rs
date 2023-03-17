// std
use std::collections::HashMap;
// crates
use rand::seq::IteratorRandom;
// internal
use super::{Committee, Layout, Overlay};
use crate::node::{
    carnot::{CarnotNode, CarnotRole},
    Node, NodeId,
};

pub enum TreeType {
    FullBinaryTree,
}

pub struct TreeSettings {
    pub tree_type: TreeType,
    pub committee_size: usize,
    pub depth: usize,
}

pub struct TreeOverlay {
    layout: Layout,
}

impl TreeOverlay {
    pub fn build_full_binary_tree(settings: &TreeSettings) -> Layout {
        let committee_count = committee_count(settings.depth);
        let node_count = committee_count * settings.committee_size;

        let mut committees = HashMap::new();
        let mut parents = HashMap::new();
        let mut children = HashMap::new();
        let mut layers = HashMap::new();

        for (committee_id, nodes) in (0..node_count)
            .collect::<Vec<usize>>()
            .chunks(settings.committee_size)
            .enumerate()
        {
            let mut has_children = false;
            let left_child_id = 2 * committee_id + 1;
            let right_child_id = left_child_id + 1;

            // Check for leaf nodes.
            if right_child_id <= committee_count {
                children.insert(committee_id, vec![left_child_id, right_child_id]);
                has_children = true;
            }

            // Root node has no parent.
            if committee_id > 0 {
                let parent_id = get_parent_id(committee_id);
                parents.insert(committee_id, parent_id);
            }

            let role = match (committee_id, has_children) {
                (0, _) => CarnotRole::Root,
                (_, true) => CarnotRole::Intermediate,
                (_, false) => CarnotRole::Leaf,
            };

            let committee = Committee {
                nodes: nodes.iter().copied().collect(),
                role,
            };

            committees.insert(committee_id, committee);

            layers
                .entry(get_layer(committee_id))
                .or_insert_with(Vec::new)
                .push(committee_id);
        }

        Layout::new(committees, parents, children, layers)
    }
}

impl Overlay<CarnotNode> for TreeOverlay {
    type Settings = TreeSettings;

    fn new(settings: Self::Settings) -> Self {
        let layout = match settings.tree_type {
            TreeType::FullBinaryTree => Self::build_full_binary_tree(&settings),
        };

        Self { layout }
    }

    fn leaders<R: rand::Rng>(
        &self,
        nodes: &[NodeId],
        size: usize,
        rng: &mut R,
    ) -> Box<dyn Iterator<Item = NodeId>> {
        let leaders = nodes.iter().copied().choose_multiple(rng, size).into_iter();
        Box::new(leaders)
    }

    fn layout<R: rand::Rng>(&self, _nodes: &[NodeId], _rng: &mut R) -> Layout {
        self.layout.clone()
    }
}

/// Returns the number of nodes in the whole tree.
/// `depth` parameter assumes that roots is included.
fn committee_count(depth: usize) -> usize {
    (1 << depth) - 1
}

fn get_parent_id(id: usize) -> usize {
    (id - 1 + id % 2) / 2
}

/// Get a layer in a tree of a given committee id.
fn get_layer(id: usize) -> usize {
    (id as f64 + 1.).log2().floor() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_full_depth_1() {
        let layout = TreeOverlay::build_full_binary_tree(&TreeSettings {
            tree_type: TreeType::FullBinaryTree,
            depth: 1,
            committee_size: 1,
        });
        assert_eq!(layout.committees.len(), 1);
        assert!(layout.children.is_empty());
        assert!(layout.parent.is_empty());
    }

    #[test]
    fn build_full_depth_3() {
        let layout = TreeOverlay::build_full_binary_tree(&TreeSettings {
            tree_type: TreeType::FullBinaryTree,
            depth: 3,
            committee_size: 1,
        });
        assert_eq!(layout.children[&0], vec![1, 2]);
        assert_eq!(layout.parent[&1], 0);
        assert_eq!(layout.parent[&2], 0);

        assert_eq!(layout.children[&1], vec![3, 4]);
        assert_eq!(layout.children[&2], vec![5, 6]);

        assert_eq!(layout.parent[&3], 1);
        assert_eq!(layout.children.get(&3), None);

        assert_eq!(layout.parent[&4], 1);
        assert_eq!(layout.children.get(&4), None);

        assert_eq!(layout.parent[&5], 2);
        assert_eq!(layout.children.get(&5), None);

        assert_eq!(layout.parent[&6], 2);
        assert_eq!(layout.children.get(&6), None);
    }

    #[test]
    fn build_full_committee_size() {
        let layout = TreeOverlay::build_full_binary_tree(&TreeSettings {
            tree_type: TreeType::FullBinaryTree,
            depth: 10,
            committee_size: 10,
        });

        // 2^h - 1
        assert_eq!(layout.committees.len(), 1023);

        let root_nodes = &layout.committees[&0].nodes;
        assert_eq!(root_nodes.len(), 10);
        assert_eq!(root_nodes.first(), Some(&0));
        assert_eq!(root_nodes.last(), Some(&9));

        let last_nodes = &layout.committees[&1022].nodes;
        assert_eq!(last_nodes.len(), 10);
        assert_eq!(last_nodes.first(), Some(&10220));
        assert_eq!(last_nodes.last(), Some(&10229));
    }

    #[test]
    fn check_committee_role() {
        let layout = TreeOverlay::build_full_binary_tree(&TreeSettings {
            tree_type: TreeType::FullBinaryTree,
            depth: 3,
            committee_size: 1,
        });

        assert_eq!(layout.committees[&0].role, CarnotRole::Root);
        assert_eq!(layout.committees[&1].role, CarnotRole::Intermediate);
        assert_eq!(layout.committees[&2].role, CarnotRole::Intermediate);
        assert_eq!(layout.committees[&3].role, CarnotRole::Leaf);
        assert_eq!(layout.committees[&6].role, CarnotRole::Leaf);
    }

    #[test]
    fn check_layers() {
        let layout = TreeOverlay::build_full_binary_tree(&TreeSettings {
            tree_type: TreeType::FullBinaryTree,
            depth: 4,
            committee_size: 1,
        });

        assert_eq!(layout.layers[&0], vec![0]);
        assert_eq!(layout.layers[&1], vec![1, 2]);
        assert_eq!(layout.layers[&2], vec![3, 4, 5, 6]);
    }
}
