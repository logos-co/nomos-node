use libp2p::PeerId;
use std::collections::HashSet;
use subnetworks_assignations::MembershipHandler;

#[derive(Clone)]
pub struct AllNeighbours {
    pub neighbours: HashSet<PeerId>,
}

impl MembershipHandler for AllNeighbours {
    type NetworkId = u32;
    type Id = PeerId;

    fn membership(&self, _self_id: &Self::Id) -> HashSet<Self::NetworkId> {
        [0].into_iter().collect()
    }

    fn is_allowed(&self, _id: &Self::Id) -> bool {
        true
    }

    fn members_of(&self, _network_id: &Self::NetworkId) -> HashSet<Self::Id> {
        self.neighbours.clone()
    }

    fn members(&self) -> HashSet<Self::Id> {
        self.neighbours.clone()
    }
}
