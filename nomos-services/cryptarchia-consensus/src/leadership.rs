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
        for coin in &mut self.coins {
            if coin.is_slot_leader(epoch_state, slot, &self.config.consensus_config) {
                tracing::debug!(
                    "leader for slot {:?}, {:?}/{:?}",
                    slot,
                    coin.value(),
                    epoch_state.total_stake()
                );
                let proof = coin.to_proof(slot);
                // TODO: import orphan proofs
                *coin = coin.evolve();
                return Some(proof);
            }
        }
        None
    }
}
