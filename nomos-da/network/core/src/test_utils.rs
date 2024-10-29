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

use crossbeam_channel::{bounded, Receiver, Sender};

/// A special-purpose multi-producer, multi-consumer(MPMC) channel to relay messages indicating whether the associated stream should be closed or not.
pub struct Indicator {
    pub sender: Sender<bool>,
    pub receiver: Receiver<bool>,
}

impl Indicator {
    pub fn new(size: usize) -> Self {
        let (sender, receiver) = bounded(size);
        Self { sender, receiver }
    }

    pub fn send(&self, message: bool) {
        self.sender.send(message).unwrap();
    }

    pub fn receive(&self) -> Option<bool> {
        self.receiver.try_recv().ok()
    }
}

impl Clone for Indicator {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            receiver: self.receiver.clone(),
        }
    }
}
