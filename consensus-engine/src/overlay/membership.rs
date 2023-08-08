// std

// crates
use rand::prelude::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

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

    pub fn shuffle<T: Clone>(elements: &mut [T], entropy: [u8; 32]) {
        let mut rng = ChaCha20Rng::from_seed(entropy);
        elements.shuffle(&mut rng);
    }
}

impl CommitteeMembership for FisherYatesShuffle {
    fn reshape_committees(&self, nodes: &mut [NodeId]) {
        FisherYatesShuffle::shuffle(nodes, self.entropy);
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FreezeMembership;

impl CommitteeMembership for FreezeMembership {
    fn reshape_committees(&self, _nodes: &mut [NodeId]) {}
}
