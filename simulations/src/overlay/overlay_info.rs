use carnot_engine::{CommitteeId, NodeId, Overlay};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap, VecDeque};

pub type Blake2bU32 = blake2::Blake2b<digest::typenum::U32>;

#[derive(Debug, Serialize)]
pub struct OverlayInfo {
    pub committees: BTreeSet<CommitteeId>,
    pub committee_sizes: HashMap<CommitteeId, usize>,
    pub edges: Vec<(CommitteeId, CommitteeId)>,
    pub next_leader: NodeId,
    pub root_id: CommitteeId,
}

pub trait OverlayInfoExt {
    fn info(&self) -> OverlayInfo;
}

impl<T: Overlay> OverlayInfoExt for T {
    fn info(&self) -> OverlayInfo {
        let mut committees = BTreeSet::new();
        let mut edges = Vec::new();
        let mut committee_sizes = HashMap::new();

        let next_leader = self.next_leader();
        let root = self.root_committee();
        let root_id = root.id::<Blake2bU32>();
        committees.insert(root_id);
        committee_sizes.insert(root_id, root.len());

        let mut queue = VecDeque::new();
        queue.push_back(root);

        while let Some(current_committee) = queue.pop_front() {
            let current_id = current_committee.id::<Blake2bU32>();

            if let Some(committee_node) = current_committee.iter().next() {
                for child in self.child_committees(*committee_node) {
                    let child_id = child.id::<Blake2bU32>();
                    committees.insert(child_id);
                    committee_sizes.insert(child_id, child.len());
                    edges.push((current_id, child_id));
                    queue.push_back(child);
                }
            }
        }

        OverlayInfo {
            committees,
            committee_sizes,
            edges,
            next_leader,
            root_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use carnot_engine::{
        overlay::{
            BranchOverlay, BranchOverlaySettings, FisherYatesShuffle, RoundRobin, TreeOverlay,
            TreeOverlaySettings,
        },
        NodeId, Overlay,
    };

    use super::*;
    const ENTROPY: [u8; 32] = [0; 32];

    #[test]
    fn tree_overlay_info() {
        let nodes: Vec<_> = (0..7).map(|i| NodeId::new([i as u8; 32])).collect();
        let overlay = TreeOverlay::new(TreeOverlaySettings {
            nodes: nodes.clone(),
            current_leader: nodes[0],
            number_of_committees: 3,
            leader: RoundRobin::new(),
            committee_membership: FisherYatesShuffle::new(ENTROPY),
            super_majority_threshold: None,
        });

        let root_committee = overlay.root_committee();
        let root_node = root_committee.iter().next().unwrap();
        let child_committees = overlay.child_committees(*root_node);
        let child1 = child_committees[0].clone();
        let child2 = child_committees[1].clone();

        let info = overlay.info();
        let info_children: Vec<&(CommitteeId, CommitteeId)> = info
            .edges
            .iter()
            .filter(|(p, _)| *p == info.root_id)
            .collect();

        assert_eq!(info.committees.len(), 3);
        assert_eq!(root_committee.id::<Blake2bU32>(), info.root_id);

        let mut info_child_iter = info_children.iter();
        let info_child1 = info_child_iter.next().map(|(_, c)| c).unwrap();
        let info_child2 = info_child_iter.next().map(|(_, c)| c).unwrap();

        assert_eq!(child1.id::<Blake2bU32>(), *info_child1);
        assert_eq!(child2.id::<Blake2bU32>(), *info_child2);

        assert_eq!(
            child1.len(),
            *info.committee_sizes.get(info_child1).unwrap()
        );
        assert_eq!(
            child2.len(),
            *info.committee_sizes.get(info_child2).unwrap()
        );
    }

    #[test]
    fn branch_overlay_info() {
        let nodes: Vec<_> = (0..7).map(|i| NodeId::new([i as u8; 32])).collect();
        let overlay = BranchOverlay::new(BranchOverlaySettings {
            nodes: nodes.clone(),
            current_leader: nodes[0],
            branch_depth: 3,
            leader: RoundRobin::new(),
            committee_membership: FisherYatesShuffle::new(ENTROPY),
        });

        let root_committee = overlay.root_committee();
        let root_node = root_committee.iter().next().unwrap();

        let info = overlay.info();
        assert_eq!(info.committees.len(), 3);
        assert_eq!(root_committee.id::<Blake2bU32>(), info.root_id);

        assert_eq!(overlay.child_committees(*root_node).len(), 1);
        let layer1 = overlay
            .child_committees(*root_node)
            .first()
            .unwrap()
            .clone();
        let layer1_node = layer1.iter().next().unwrap();

        assert_eq!(overlay.child_committees(*layer1_node).len(), 1);
        let info_layer1: Vec<&(CommitteeId, CommitteeId)> = info
            .edges
            .iter()
            .filter(|(p, _)| *p == info.root_id)
            .collect();
        assert_eq!(info_layer1.len(), 1);
        let info_layer1 = info_layer1.first().map(|(_, c)| c).unwrap();

        assert_eq!(layer1.id::<Blake2bU32>(), *info_layer1);
        assert_eq!(
            layer1.len(),
            *info.committee_sizes.get(info_layer1).unwrap()
        );

        let layer2 = overlay
            .child_committees(*layer1_node)
            .first()
            .unwrap()
            .clone();
        let layer2_node = layer2.iter().next().unwrap();
        assert_eq!(overlay.child_committees(*layer2_node).len(), 0);

        let info_layer2: Vec<&(CommitteeId, CommitteeId)> = info
            .edges
            .iter()
            .filter(|(p, _)| *p == layer1.id::<Blake2bU32>())
            .collect();

        assert_eq!(info_layer2.len(), 1);
        let info_layer2 = info_layer2.first().map(|(_, c)| c).unwrap();

        assert_eq!(layer2.id::<Blake2bU32>(), *info_layer2);
        assert_eq!(
            layer2.len(),
            *info.committee_sizes.get(info_layer2).unwrap()
        );
    }
}
