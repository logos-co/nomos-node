use cryptarchia_engine::Slot;
use cryptarchia_ledger::{Coin, Commitment, Config, EpochState, LeaderProof};
use nomos_core::header::HeaderId;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Leader {
    coins: HashMap<HeaderId, Vec<Coin>>,
    config: cryptarchia_ledger::Config,
}

impl Leader {
    pub fn new(genesis: HeaderId, coins: Vec<Coin>, config: Config) -> Self {
        Leader {
            coins: HashMap::from([(genesis, coins)]),
            config,
        }
    }

    // Signal that the chain extended with a new header, possibly evolving a leader coin in the process
    // FIXME: this implementation does not delete old coins and will attempt to re-use a coin in different forks,
    //        we should use the orphan proofs mechanism to handle this.
    pub fn follow_chain(&mut self, parent_id: HeaderId, id: HeaderId, to_evolve: Commitment) {
        if let Some(coins) = self.coins.get(&parent_id) {
            let coins = coins
                .iter()
                .map(|coin| {
                    if coin.commitment() == to_evolve {
                        coin.evolve()
                    } else {
                        *coin
                    }
                })
                .collect();
            self.coins.insert(id, coins);
        }
    }

    pub fn build_proof_for(
        &self,
        epoch_state: &EpochState,
        slot: Slot,
        parent: HeaderId,
    ) -> Option<LeaderProof> {
        let coins = self.coins.get(&parent)?;
        for coin in coins {
            if coin.is_slot_leader(epoch_state, slot, &self.config.consensus_config) {
                tracing::debug!(
                    "leader for slot {:?}, {:?}/{:?}",
                    slot,
                    coin.value(),
                    epoch_state.total_stake()
                );
                let proof = coin.to_proof(slot);
                return Some(proof);
            }
        }
        None
    }
}
