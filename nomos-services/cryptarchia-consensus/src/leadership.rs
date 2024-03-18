use cryptarchia_engine::Slot;
use cryptarchia_ledger::{Coin, Config, EpochState, LeaderProof};

pub struct Leader {
    coins: Vec<Coin>,
    config: cryptarchia_ledger::Config,
}

impl Leader {
    pub fn new(coins: Vec<Coin>, config: Config) -> Self {
        Leader { coins, config }
    }

    pub fn build_proof_for(&mut self, epoch_state: &EpochState, slot: Slot) -> Option<LeaderProof> {
        for coin in &self.coins {
            if coin.is_slot_leader(epoch_state, slot, &self.config.consensus_config) {
                return Some(coin.to_proof(slot));
            }
        }
        None
    }
}
