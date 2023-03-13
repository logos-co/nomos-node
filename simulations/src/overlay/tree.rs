use std::collections::HashMap;

use crate::node::NodeId;

use super::{Committee, Layout, Overlay};

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

        for (committee_id, nodes) in (0..node_count)
            .collect::<Vec<usize>>()
            .chunks(settings.committee_size)
            .enumerate()
        {
            committees.insert(committee_id, nodes.iter().copied().collect::<Committee>());
            let left_child_id = 2 * committee_id + 1;
            let right_child_id = left_child_id + 1;

            // Check for leaf nodes.
            if right_child_id <= committee_count {
                children.insert(committee_id, vec![left_child_id, right_child_id]);
            }

            // Root node has no parent.
            if committee_id > 0 {
                let parent_id = get_parent_id(committee_id);
                parents.insert(committee_id, parent_id);
            }
        }

        Layout::new(committees, parents, children)
    }
}

impl Overlay for TreeOverlay {
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
        _size: usize,
        _rng: &mut R,
    ) -> Box<dyn Iterator<Item = NodeId>> {
        // Assuming a first node is always a leader.
        Box::new(nodes.first().copied().into_iter())
    }

    fn layout<R: rand::Rng>(&self, _nodes: &[NodeId], _rng: &mut R) -> Layout {
        self.layout.clone()
    }
}

fn committee_count(depth: usize) -> usize {
    ((2u32.pow(depth as u32 + 1) - 1) / 2) as usize
}

fn get_parent_id(id: usize) -> usize {
    (id - 1 + id % 2) / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_full_binary_tree_depth_1() {
        let settings = TreeSettings {
            tree_type: TreeType::FullBinaryTree,
            depth: 1,
            committee_size: 1,
        };
        let layout = TreeOverlay::build_full_binary_tree(&settings);
        assert_eq!(layout.committees.len(), 1);
        assert_eq!(layout.children.len(), 1);
        assert!(layout.parent.is_empty());
    }

    #[test]
    fn test_build_full_binary_tree_depth_2() {
        let settings = TreeSettings {
            tree_type: TreeType::FullBinaryTree,
            depth: 2,
            committee_size: 1,
        };
        let layout = TreeOverlay::build_full_binary_tree(&settings);
        assert_eq!(layout.children[&0], vec![1, 2]);
        assert_eq!(layout.parent[&1], 0);
        assert_eq!(layout.parent[&2], 0);
    }
}
