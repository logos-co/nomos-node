// std
use std::collections::HashMap;
// crates
use rand::seq::IteratorRandom;
use serde::Deserialize;
// internal
use super::{Committee, Layout, Overlay};
use crate::node::{CommitteeId, NodeId};

#[derive(Clone, Deserialize)]
pub enum TreeType {
    FullBinaryTree,
}

#[derive(Clone, Deserialize)]
pub struct TreeSettings {
    pub tree_type: TreeType,
    pub committee_size: usize,
    pub depth: usize,
}

pub struct TreeOverlay {
    settings: TreeSettings,
}

struct TreeProperties {
    committee_count: usize,
    node_count: usize,
}

impl TreeOverlay {
    fn build_full_binary_tree<R: rand::Rng>(
        node_ids: &[NodeId],
        rng: &mut R,
        settings: &TreeSettings,
    ) -> Layout {
        let properties = get_tree_properties(settings);

        // For full binary tree to be formed from existing nodes
        // a certain unique node count needs to be provided.
        assert!(properties.node_count <= node_ids.len());

        let mut committees = HashMap::new();
        let mut parents = HashMap::new();
        let mut children = HashMap::new();
        let mut layers = HashMap::new();

        for (committee_id, nodes) in node_ids
            .iter()
            .choose_multiple(rng, properties.node_count)
            .chunks(settings.committee_size)
            .enumerate()
        {
            // TODO: Why do we have has_children here?
            let mut _has_children = false;
            let left_child_id = 2 * committee_id + 1;
            let right_child_id = left_child_id + 1;

            // Check for leaf nodes.
            if right_child_id <= properties.committee_count {
                children.insert(
                    committee_id.into(),
                    vec![left_child_id.into(), right_child_id.into()],
                );
                _has_children = true;
            }

            // Root node has no parent.
            if committee_id > 0 {
                let parent_id = get_parent_id(committee_id);
                parents.insert(committee_id.into(), parent_id.into());
            }

            let committee = Committee {
                nodes: nodes.iter().copied().copied().collect(),
            };

            committees.insert(committee_id.into(), committee);

            layers
                .entry(get_layer(committee_id))
                .or_insert_with(Vec::new)
                .push(committee_id.into());
        }

        Layout::new(committees, parents, children, layers)
    }
}

impl Overlay for TreeOverlay {
    type Settings = TreeSettings;

    fn new(settings: Self::Settings) -> Self {
        Self { settings }
    }

    fn nodes(&self) -> Vec<NodeId> {
        let properties = get_tree_properties(&self.settings);
        (0..properties.node_count).map(From::from).collect()
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

    fn layout<R: rand::Rng>(&self, nodes: &[NodeId], rng: &mut R) -> Layout {
        match self.settings.tree_type {
            TreeType::FullBinaryTree => Self::build_full_binary_tree(nodes, rng, &self.settings),
        }
    }
}

fn get_tree_properties(settings: &TreeSettings) -> TreeProperties {
    let committee_count = committee_count(settings.depth);
    let node_count = committee_count * settings.committee_size;
    TreeProperties {
        committee_count,
        node_count,
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
fn get_layer(id: usize) -> CommitteeId {
    CommitteeId::new((id as f64 + 1.).log2().floor() as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::mock::StepRng;

    #[test]
    fn build_full_depth_1() {
        let mut rng = StepRng::new(1, 0);
        let overlay = TreeOverlay::new(TreeSettings {
            tree_type: TreeType::FullBinaryTree,
            depth: 1,
            committee_size: 1,
        });
        let nodes = overlay.nodes();
        let layout = overlay.layout(&nodes, &mut rng);
        assert_eq!(layout.committees.len(), 1);
        assert!(layout.children.is_empty());
        assert!(layout.parent.is_empty());
    }

    #[test]
    fn build_full_depth_3() {
        let mut rng = StepRng::new(1, 0);
        let overlay = TreeOverlay::new(TreeSettings {
            tree_type: TreeType::FullBinaryTree,
            depth: 3,
            committee_size: 1,
        });
        let nodes = overlay.nodes();
        let layout = overlay.layout(&nodes, &mut rng);
        assert_eq!(
            layout.children[&CommitteeId::new(0)],
            vec![1.into(), 2.into()]
        );
        assert_eq!(layout.parent[&CommitteeId::new(1)], 0.into());
        assert_eq!(layout.parent[&CommitteeId::new(2)], 0.into());

        assert_eq!(
            layout.children[&CommitteeId::new(1)],
            vec![3.into(), 4.into()]
        );
        assert_eq!(
            layout.children[&CommitteeId::new(2)],
            vec![5.into(), 6.into()]
        );

        assert_eq!(layout.parent[&CommitteeId::new(3)], 1.into());
        assert_eq!(layout.children.get(&CommitteeId::new(3)), None);

        assert_eq!(layout.parent[&CommitteeId::new(4)], 1.into());
        assert_eq!(layout.children.get(&CommitteeId::new(4)), None);

        assert_eq!(layout.parent[&CommitteeId::new(5)], 2.into());
        assert_eq!(layout.children.get(&CommitteeId::new(5)), None);

        assert_eq!(layout.parent[&CommitteeId::new(6)], 2.into());
        assert_eq!(layout.children.get(&CommitteeId::new(6)), None);
    }

    #[test]
    fn build_full_committee_size() {
        let mut rng = StepRng::new(1, 0);
        let overlay = TreeOverlay::new(TreeSettings {
            tree_type: TreeType::FullBinaryTree,
            depth: 10,
            committee_size: 10,
        });
        let nodes = overlay.nodes();
        let layout = overlay.layout(&nodes, &mut rng);

        // 2^h - 1
        assert_eq!(layout.committees.len(), 1023);

        let root_nodes = &layout.committees[&CommitteeId::new(0)].nodes;
        assert_eq!(root_nodes.len(), 10);
        assert_eq!(root_nodes.first(), Some(&NodeId::new(0)));
        assert_eq!(root_nodes.last(), Some(&NodeId::new(9)));

        let last_nodes = &layout.committees[&CommitteeId::new(1022)].nodes;
        assert_eq!(last_nodes.len(), 10);
        assert_eq!(last_nodes.first(), Some(&NodeId::new(10220)));
        assert_eq!(last_nodes.last(), Some(&NodeId::new(10229)));
    }

    #[test]
    fn check_committee_role() {
        let mut rng = StepRng::new(1, 0);
        let overlay = TreeOverlay::new(TreeSettings {
            tree_type: TreeType::FullBinaryTree,
            depth: 3,
            committee_size: 1,
        });
        let nodes = overlay.nodes();
        let layout = overlay.layout(&nodes, &mut rng);
    }

    #[test]
    fn check_layers() {
        let mut rng = StepRng::new(1, 0);
        let overlay = TreeOverlay::new(TreeSettings {
            tree_type: TreeType::FullBinaryTree,
            depth: 4,
            committee_size: 1,
        });
        let nodes = overlay.nodes();
        let layout = overlay.layout(&nodes, &mut rng);
        assert_eq!(layout.layers[&CommitteeId::new(0)], vec![0.into()]);
        assert_eq!(
            layout.layers[&CommitteeId::new(1)],
            vec![1.into(), 2.into()]
        );
        assert_eq!(
            layout.layers[&CommitteeId::new(2)],
            vec![3.into(), 4.into(), 5.into(), 6.into()]
        );
    }
}
