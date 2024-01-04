use carnot_engine::overlay::RoundRobin;
use carnot_engine::{
    overlay::{Error as RandomBeaconError, LeaderSelection, RandomBeaconState},
    TimeoutQc,
};
use nomos_core::block::Block;
use std::{convert::Infallible, error::Error, hash::Hash};

pub trait UpdateableLeaderSelection: LeaderSelection {
    type Error: Error;

    fn on_new_block_received<Tx: Hash + Clone + Eq, Blob: Clone + Eq + Hash>(
        &self,
        block: &Block<Tx, Blob>,
    ) -> Result<Self, Self::Error>;
    fn on_timeout_qc_received(&self, qc: &TimeoutQc) -> Result<Self, Self::Error>;
}

impl UpdateableLeaderSelection for RoundRobin {
    type Error = Infallible;

    fn on_new_block_received<Tx: Hash + Clone + Eq, Blob: Clone + Eq + Hash>(
        &self,
        _block: &Block<Tx, Blob>,
    ) -> Result<Self, Self::Error> {
        Ok(self.advance())
    }

    fn on_timeout_qc_received(&self, _qc: &TimeoutQc) -> Result<Self, Self::Error> {
        Ok(self.advance())
    }
}

impl UpdateableLeaderSelection for RandomBeaconState {
    type Error = RandomBeaconError;

    fn on_new_block_received<Tx: Hash + Clone + Eq, Blob: Clone + Eq + Hash>(
        &self,
        block: &Block<Tx, Blob>,
    ) -> Result<Self, Self::Error> {
        self.check_advance_happy(block.beacon().clone(), block.header().parent_qc.view())
        // TODO: check random beacon public keys is leader id
    }

    fn on_timeout_qc_received(&self, qc: &TimeoutQc) -> Result<Self, Self::Error> {
        Ok(Self::generate_sad(qc.view(), self))
    }
}
