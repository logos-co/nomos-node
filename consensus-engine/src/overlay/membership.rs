// std

// crates
use rand::prelude::{SliceRandom, StdRng};
use rand::SeedableRng;

// internal
use crate::overlay::CommitteeMembership;
use crate::NodeId;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FisherYatesShuffle {
    entropy: [u8; 32],
}

impl FisherYatesShuffle {
    pub fn new(entropy: [u8; 32]) -> Self {
        Self { entropy }
    }

    pub fn shuffle<T: Clone>(elements: &[T], entropy: [u8; 32]) -> Vec<T> {
        let mut elements = elements.to_vec();
        let mut rng = StdRng::from_seed(entropy);
        elements.shuffle(&mut rng);
        elements
    }
}

impl CommitteeMembership for FisherYatesShuffle {
    fn reshape_committees(&self, nodes: &[NodeId]) -> Vec<NodeId> {
        FisherYatesShuffle::shuffle(nodes, self.entropy)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FreezeMembership;

impl CommitteeMembership for FreezeMembership {
    fn reshape_committees(&self, nodes: &[NodeId]) -> Vec<NodeId> {
        nodes.to_vec()
    }
}
