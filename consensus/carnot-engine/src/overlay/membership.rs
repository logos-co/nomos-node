use nomos_utils::fisheryates::FisherYatesShuffle;

use crate::{overlay::CommitteeMembership, NodeId};

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
