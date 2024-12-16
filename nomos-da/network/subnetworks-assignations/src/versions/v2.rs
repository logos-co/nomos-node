use crate::MembershipHandler;
use libp2p_identity::PeerId;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct FillWithOriginalReplication {
    pub assignations: Vec<HashSet<PeerId>>,
    pub subnetwork_size: usize,
    pub dispersal_factor: usize,
    pub original_replication: usize,
    pub pivot: u16,
}

impl FillWithOriginalReplication {
    pub fn new(
        peers: &[PeerId],
        subnetwork_size: usize,
        dispersal_factor: usize,
        original_replication: usize,
        pivot: u16,
    ) -> Self {
        Self {
            assignations: Self::fill(
                peers,
                subnetwork_size,
                dispersal_factor,
                original_replication,
                pivot,
            ),
            subnetwork_size,
            dispersal_factor,
            original_replication,
            pivot,
        }
    }
    fn fill(
        peers: &[PeerId],
        subnetwork_size: usize,
        dispersal_factor: usize,
        original_replication: usize,
        pivot: u16,
    ) -> Vec<HashSet<PeerId>> {
        assert!(!peers.is_empty());
        // sort list to make it deterministic
        let mut peers = peers.to_vec();
        peers.sort_unstable();
        // take n peers and fill a subnetwork until all subnetworks are filled
        let mut cycle = peers.into_iter().cycle();
        (0..subnetwork_size)
            .map(|subnetwork| {
                (0..{
                    // choose factor depending on if it is in the original size of the encoding or not
                    if subnetwork < pivot as usize {
                        original_replication
                    } else {
                        dispersal_factor
                    }
                })
                    .map(|_| cycle.next().unwrap())
                    .collect()
            })
            .collect()
    }
}

impl MembershipHandler for FillWithOriginalReplication {
    type NetworkId = u16;
    type Id = PeerId;

    fn membership(&self, id: &Self::Id) -> HashSet<Self::NetworkId> {
        self.assignations
            .iter()
            .enumerate()
            .filter_map(|(netowrk_id, subnetwork)| {
                subnetwork
                    .contains(id)
                    .then_some(netowrk_id as Self::NetworkId)
            })
            .collect()
    }

    fn is_allowed(&self, id: &Self::Id) -> bool {
        for subnetwork in &self.assignations {
            if subnetwork.contains(id) {
                return true;
            }
        }
        false
    }

    fn members_of(&self, network_id: &Self::NetworkId) -> HashSet<Self::Id> {
        self.assignations[*network_id as usize].clone()
    }

    fn members(&self) -> HashSet<Self::Id> {
        self.assignations.iter().flatten().copied().collect()
    }
}

#[cfg(test)]
mod test {
    use crate::versions::v2::FillWithOriginalReplication;
    use libp2p_identity::PeerId;

    #[test]
    fn test_distribution_fill_with_original_replication_from_node_list() {
        let nodes: Vec<_> = std::iter::repeat_with(PeerId::random).take(100).collect();
        let dispersal_factor = 2;
        let subnetwork_size = 1024;
        let original_replication = 10;
        let pivot = 512;
        let distribution = FillWithOriginalReplication::new(
            &nodes,
            subnetwork_size,
            dispersal_factor,
            original_replication,
            pivot,
        );
        assert_eq!(distribution.assignations.len(), subnetwork_size);
        for subnetwork in &distribution.assignations[..pivot as usize] {
            assert_eq!(subnetwork.len(), original_replication);
        }
        for subnetwork in &distribution.assignations[pivot as usize..] {
            assert_eq!(subnetwork.len(), dispersal_factor);
        }
    }
}
