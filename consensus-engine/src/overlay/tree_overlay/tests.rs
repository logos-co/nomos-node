use super::tree::Tree;
use crate::overlay::RoundRobin;
use crate::Overlay;

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
        current_leader: nodes[0],
        nodes,
        entropy: [0; 32],
        number_of_committees: 3,
        leader: RoundRobin::new(),
    });

    let mut expected_root = HashSet::new();
    expected_root.insert(overlay.nodes[9]);
    expected_root.extend(overlay.nodes[0..3].iter());

    assert_eq!(overlay.root_committee(), expected_root);
}

#[test]
fn test_leaf_committees() {
    let nodes: Vec<[u8; 32]> = (0..10).map(|i| [i as u8; 32]).collect();
    let overlay = TreeOverlay::new(TreeOverlaySettings {
        current_leader: nodes[0],
        nodes,
        entropy: [0; 32],
        number_of_committees: 3,
        leader: RoundRobin::new(),
    });

    let mut leaf_committees = overlay
        .leaf_committees([0; 32])
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
    let nodes: Vec<[u8; 32]> = (0..10).map(|i| [i as u8; 32]).collect();
    let overlay = TreeOverlay::new(TreeOverlaySettings {
        current_leader: nodes[0],
        nodes,
        entropy: [0; 32],
        number_of_committees: 3,
        leader: RoundRobin::new(),
    });

    assert_eq!(overlay.super_majority_threshold(overlay.nodes[8]), 0);
}

#[test]
fn test_super_majority_threshold_for_root_member() {
    let nodes: Vec<[u8; 32]> = (0..10).map(|i| [i as u8; 32]).collect();
    let overlay = TreeOverlay::new(TreeOverlaySettings {
        current_leader: nodes[0],
        nodes,
        entropy: [0; 32],
        number_of_committees: 3,
        leader: RoundRobin::new(),
    });

    assert_eq!(overlay.super_majority_threshold(overlay.nodes[0]), 3);
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
