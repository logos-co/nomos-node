// std

// crates
use rand::prelude::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use shuffle::shuffler::Shuffler;

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
        // Implementation of fisher yates shuffling
        // https://en.wikipedia.org/wiki/Fisher%E2%80%93Yates_shuffle
        for i in (1..elements.len()).rev() {
            let j = rng.gen_range(0..(i + 1));
            elements.swap(i, j);
        }
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
