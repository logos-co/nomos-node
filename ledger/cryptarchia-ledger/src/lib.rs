mod coin;
mod config;
mod crypto;
mod leader_proof;
mod nonce;
mod utils;

use blake2::Digest;
use cryptarchia_engine::{Epoch, Slot};
use crypto::Blake2b;
use std::{collections::HashMap, hash::Hash};
use thiserror::Error;

type HashTrieSet<T> = rpds::HashTrieSetSync<T>;

pub use coin::{Coin, Value};
pub use config::Config;
pub use leader_proof::*;
pub use nonce::*;

#[derive(Clone, Debug, Error)]
pub enum LedgerError<Id> {
    #[error("Commitment not found in the ledger state")]
    CommitmentNotFound,
    #[error("Nullifier already exists in the ledger state")]
    NullifierExists,
    #[error("Commitment already exists in the ledger state")]
    CommitmentExists,
    #[error("Invalid block slot {block:?} for parent slot {parent:?}")]
    InvalidSlot { parent: Slot, block: Slot },
    #[error("Parent block not found: {0:?}")]
    ParentNotFound(Id),
    #[error("Orphan block missing: {0:?}. Importing leader proofs requires the block to be validated first")]
    OrphanMissing(Id),
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EpochState {
    // The epoch this snapshot is for
    epoch: Epoch,
    // value of the ledger nonce after 'epoch_period_nonce_buffer' slots from the beginning of the epoch
    nonce: Nonce,
    // stake distribution snapshot taken at the beginning of the epoch
    // (in practice, this is equivalent to the coins the are spendable at the beginning of the epoch)
    commitments: HashTrieSet<Commitment>,
    total_stake: Value,
}

impl EpochState {
    fn update_from_ledger(self, ledger: &LedgerState, config: &Config) -> Self {
        let nonce_snapshot_slot = config.nonce_snapshot(self.epoch);
        let nonce = if ledger.slot < nonce_snapshot_slot {
            ledger.nonce
        } else {
            self.nonce
        };

        let stake_snapshot_slot = config.stake_distribution_snapshot(self.epoch);
        let commitments = if ledger.slot < stake_snapshot_slot {
            ledger.lead_commitments.clone()
        } else {
            self.commitments
        };
        Self {
            epoch: self.epoch,
            nonce,
            commitments,
            total_stake: self.total_stake,
        }
    }

    fn is_eligible_leader(&self, commitment: &Commitment) -> bool {
        self.commitments.contains(commitment)
    }

    pub fn epoch(&self) -> Epoch {
        self.epoch
    }

    pub fn total_stake(&self) -> Value {
        self.total_stake
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ledger<Id: Eq + Hash> {
    states: HashMap<Id, LedgerState>,
    config: Config,
}

impl<Id> Ledger<Id>
where
    Id: Eq + Hash + Copy,
{
    pub fn from_genesis(id: Id, state: LedgerState, config: Config) -> Self {
        Self {
            states: [(id, state)].into_iter().collect(),
            config,
        }
    }

    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub fn try_update(
        &self,
        id: Id,
        parent_id: Id,
        slot: Slot,
        proof: &LeaderProof,
        // (update corresponding to the leader proof, leader proof)
        orphan_proofs: impl IntoIterator<Item = (Id, LeaderProof)>,
    ) -> Result<Self, LedgerError<Id>> {
        let parent_state = self
            .states
            .get(&parent_id)
            .ok_or(LedgerError::ParentNotFound(parent_id))?;
        let config = self.config.clone();

        // Oprhan proofs need to be:
        // * locally valid for the block they were originally in
        // * not in conflict with the current ledger state
        // This first condition is checked here, the second one is checked in the state update
        // (in particular, we do not check the imported leader proof is for an earlier slot)
        let (orphan_ids, orphan_proofs): (Vec<_>, Vec<_>) = orphan_proofs.into_iter().unzip();
        for orphan_id in orphan_ids {
            if !self.states.contains_key(&orphan_id) {
                return Err(LedgerError::OrphanMissing(orphan_id));
            }
        }

        let new_state =
            parent_state
                .clone()
                .try_update(slot, proof, &orphan_proofs, &self.config)?;

        let mut states = self.states.clone();

        states.insert(id, new_state);

        Ok(Self { states, config })
    }

    pub fn state(&self, id: &Id) -> Option<&LedgerState> {
        self.states.get(id)
    }

    pub fn config(&self) -> &Config {
        &self.config
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Eq, PartialEq)]
pub struct LedgerState {
    // commitments to coins that can be used to propose new blocks
    lead_commitments: HashTrieSet<Commitment>,
    // commitments to coins that can be spent, this is a superset of lead_commitments
    spend_commitments: HashTrieSet<Commitment>,
    nullifiers: HashTrieSet<Nullifier>,
    // randomness contribution
    nonce: Nonce,
    slot: Slot,
    // rolling snapshot of the state for the next epoch, used for epoch transitions
    next_epoch_state: EpochState,
    epoch_state: EpochState,
}

impl LedgerState {
    fn try_update<Id>(
        self,
        slot: Slot,
        proof: &LeaderProof,
        orphan_proofs: &[LeaderProof],
        config: &Config,
    ) -> Result<Self, LedgerError<Id>> {
        // TODO: import leader proofs
        self.update_epoch_state(slot, config)?
            .try_apply_leadership(proof, orphan_proofs, config)
    }

    fn update_epoch_state<Id>(self, slot: Slot, config: &Config) -> Result<Self, LedgerError<Id>> {
        if slot <= self.slot {
            return Err(LedgerError::InvalidSlot {
                parent: self.slot,
                block: slot,
            });
        }

        // TODO: update once supply can change
        let total_stake = self.epoch_state.total_stake;
        let current_epoch = config.epoch(self.slot);
        let new_epoch = config.epoch(slot);

        // there are 3 cases to consider:
        // 1. we are in the same epoch as the parent state
        //    update the next epoch state
        // 2. we are in the next epoch
        //    use the next epoch state as the current epoch state and reset next epoch state
        // 3. we are in the next-next or later epoch:
        //    use the parent state as the epoch state and reset next epoch state
        if current_epoch == new_epoch {
            // case 1)
            let next_epoch_state = self
                .next_epoch_state
                .clone()
                .update_from_ledger(&self, config);
            Ok(Self {
                slot,
                next_epoch_state,
                ..self
            })
        } else if new_epoch == current_epoch + 1 {
            // case 2)
            let epoch_state = self.next_epoch_state.clone();
            let next_epoch_state = EpochState {
                epoch: new_epoch + 1,
                nonce: self.nonce,
                commitments: self.spend_commitments.clone(),
                total_stake,
            };
            Ok(Self {
                slot,
                next_epoch_state,
                epoch_state,
                ..self
            })
        } else {
            // case 3)
            let epoch_state = EpochState {
                epoch: new_epoch,
                nonce: self.nonce,
                commitments: self.spend_commitments.clone(),
                total_stake,
            };
            let next_epoch_state = EpochState {
                epoch: new_epoch + 1,
                nonce: self.nonce,
                commitments: self.spend_commitments.clone(),
                total_stake,
            };
            Ok(Self {
                slot,
                next_epoch_state,
                epoch_state,
                ..self
            })
        }
    }

    fn try_apply_proof<Id>(
        self,
        proof: &LeaderProof,
        config: &Config,
    ) -> Result<Self, LedgerError<Id>> {
        assert_eq!(config.epoch(proof.slot()), self.epoch_state.epoch);
        // The leadership coin either has to be in the state snapshot or be derived from
        // a coin that is in the state snapshot (i.e. be in the lead coins commitments)
        if !self.can_lead(proof.commitment())
            && !self.epoch_state.is_eligible_leader(proof.commitment())
        {
            return Err(LedgerError::CommitmentNotFound);
        }

        if self.is_nullified(proof.nullifier()) {
            return Err(LedgerError::NullifierExists);
        }

        if self.is_committed(proof.evolved_commitment()) {
            return Err(LedgerError::CommitmentExists);
        }

        let lead_commitments = self.lead_commitments.insert(*proof.evolved_commitment());
        let spend_commitments = self.spend_commitments.insert(*proof.evolved_commitment());
        let nullifiers = self.nullifiers.insert(*proof.nullifier());

        Ok(Self {
            lead_commitments,
            spend_commitments,
            nullifiers,
            ..self
        })
    }

    fn try_apply_leadership<Id>(
        mut self,
        proof: &LeaderProof,
        orphan_proofs: &[LeaderProof],
        config: &Config,
    ) -> Result<Self, LedgerError<Id>> {
        for proof in orphan_proofs {
            self = self.try_apply_proof(proof, config)?;
        }

        self = self.try_apply_proof(proof, config)?.update_nonce(proof);

        Ok(self)
    }

    pub fn can_spend(&self, commitment: &Commitment) -> bool {
        self.spend_commitments.contains(commitment)
    }

    pub fn can_lead(&self, commitment: &Commitment) -> bool {
        self.lead_commitments.contains(commitment)
    }

    pub fn is_nullified(&self, nullifier: &Nullifier) -> bool {
        self.nullifiers.contains(nullifier)
    }

    pub fn is_committed(&self, commitment: &Commitment) -> bool {
        // spendable coins are a superset of coins that can lead, so it's sufficient to check only one set
        self.spend_commitments.contains(commitment)
    }

    fn update_nonce(self, proof: &LeaderProof) -> Self {
        Self {
            nonce: <[u8; 32]>::from(
                Blake2b::new_with_prefix("epoch-nonce".as_bytes())
                    .chain_update(<[u8; 32]>::from(self.nonce))
                    .chain_update(proof.nullifier())
                    .chain_update(proof.slot().to_be_bytes())
                    .finalize(),
            )
            .into(),
            ..self
        }
    }

    pub fn from_commitments(
        commitments: impl IntoIterator<Item = Commitment>,
        total_stake: Value,
    ) -> Self {
        let commitments = commitments.into_iter().collect::<HashTrieSet<_>>();
        Self {
            lead_commitments: commitments.clone(),
            spend_commitments: commitments,
            nullifiers: Default::default(),
            nonce: [0; 32].into(),
            slot: 0.into(),
            next_epoch_state: EpochState {
                epoch: 1.into(),
                nonce: [0; 32].into(),
                commitments: Default::default(),
                total_stake,
            },
            epoch_state: EpochState {
                epoch: 0.into(),
                nonce: [0; 32].into(),
                commitments: Default::default(),
                total_stake,
            },
        }
    }

    pub fn slot(&self) -> Slot {
        self.slot
    }

    pub fn epoch_state(&self) -> &EpochState {
        &self.epoch_state
    }

    pub fn next_epoch_state(&self) -> &EpochState {
        &self.next_epoch_state
    }
}

impl core::fmt::Debug for LedgerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LedgerState")
            .field(
                "lead_commitment",
                &self.lead_commitments.iter().collect::<Vec<_>>(),
            )
            .field(
                "spend_commitments",
                &self.spend_commitments.iter().collect::<Vec<_>>(),
            )
            .field("nullifiers", &self.nullifiers.iter().collect::<Vec<_>>())
            .field("nonce", &self.nonce)
            .field("slot", &self.slot)
            .finish()
    }
}

#[cfg(test)]
pub mod tests {
    use super::{Coin, EpochState, LeaderProof, Ledger, LedgerState, Nullifier};
    use crate::{crypto::Blake2b, Commitment, Config, LedgerError};
    use blake2::Digest;
    use cryptarchia_engine::Slot;
    use serde_test::{assert_tokens, Configure, Token};

    type HeaderId = [u8; 32];

    fn coin(id: u64) -> Coin {
        let mut sk = [0; 32];
        sk[..8].copy_from_slice(&id.to_be_bytes());
        Coin::new(sk, [0; 32].into(), 1.into())
    }

    fn update_ledger(
        ledger: &mut Ledger<HeaderId>,
        parent: HeaderId,
        slot: impl Into<Slot>,
        coin: Coin,
    ) -> Result<HeaderId, LedgerError<HeaderId>> {
        update_orphans(ledger, parent, slot, coin, vec![])
    }

    fn make_id(parent: HeaderId, slot: impl Into<Slot>, coin: Coin) -> HeaderId {
        Blake2b::new()
            .chain_update(parent)
            .chain_update(slot.into().to_be_bytes())
            .chain_update(coin.vrf([0; 32].into(), 0.into()))
            .finalize()
            .into()
    }

    fn update_orphans(
        ledger: &mut Ledger<HeaderId>,
        parent: HeaderId,
        slot: impl Into<Slot>,
        coin: Coin,
        orphans: Vec<(HeaderId, (u64, Coin))>,
    ) -> Result<HeaderId, LedgerError<HeaderId>> {
        let slot = slot.into();
        let id = make_id(parent, slot, coin);
        *ledger = ledger.try_update(
            id,
            parent,
            slot,
            &coin.to_proof(slot),
            orphans
                .into_iter()
                .map(|(id, (slot, coin))| (id, coin.to_proof(slot.into()))),
        )?;
        Ok(id)
    }

    pub fn config() -> Config {
        Config {
            epoch_stake_distribution_stabilization: 4,
            epoch_period_nonce_buffer: 3,
            epoch_period_nonce_stabilization: 3,
            consensus_config: cryptarchia_engine::Config {
                security_param: 1,
                active_slot_coeff: 1.0,
            },
        }
    }

    pub fn genesis_state(commitments: &[Commitment]) -> LedgerState {
        LedgerState {
            lead_commitments: commitments.iter().cloned().collect(),
            spend_commitments: commitments.iter().cloned().collect(),
            nullifiers: Default::default(),
            nonce: [0; 32].into(),
            slot: 0.into(),
            next_epoch_state: EpochState {
                epoch: 1.into(),
                nonce: [0; 32].into(),
                commitments: commitments.iter().cloned().collect(),
                total_stake: 1.into(),
            },
            epoch_state: EpochState {
                epoch: 0.into(),
                nonce: [0; 32].into(),
                commitments: commitments.iter().cloned().collect(),
                total_stake: 1.into(),
            },
        }
    }

    fn ledger(commitments: &[Commitment]) -> (Ledger<HeaderId>, HeaderId) {
        let genesis_state = genesis_state(commitments);

        (
            Ledger::from_genesis([0; 32], genesis_state, config()),
            [0; 32],
        )
    }

    fn apply_and_add_coin(
        ledger: &mut Ledger<HeaderId>,
        parent: HeaderId,
        slot: impl Into<Slot>,
        coin_proof: Coin,
        coin_add: Coin,
    ) -> HeaderId {
        let id = update_ledger(ledger, parent, slot, coin_proof).unwrap();
        // we still don't have transactions, so the only way to add a commitment to spendable commitments and
        // test epoch snapshotting is by doing this manually
        let mut block_state = ledger.states[&id].clone();
        block_state.spend_commitments = block_state.spend_commitments.insert(coin_add.commitment());
        ledger.states.insert(id, block_state);
        id
    }

    #[test]
    fn test_ledger_state_prevents_coin_reuse() {
        let coin = coin(0);
        let (mut ledger, genesis) = ledger(&[coin.commitment()]);

        let h = update_ledger(&mut ledger, genesis, 1, coin).unwrap();

        // reusing the same coin should be prevented
        assert!(matches!(
            update_ledger(&mut ledger, h, 2, coin),
            Err(LedgerError::NullifierExists),
        ));
    }

    #[test]
    fn test_ledger_state_uncommited_coin() {
        let coin = coin(0);
        let (mut ledger, genesis) = ledger(&[]);
        assert!(matches!(
            update_ledger(&mut ledger, genesis, 1, coin),
            Err(LedgerError::CommitmentNotFound),
        ));
    }

    #[test]
    fn test_ledger_state_is_properly_updated_on_reorg() {
        let coin_1 = coin(0);
        let coin_2 = coin(1);
        let coin_3 = coin(2);

        let (mut ledger, genesis) = ledger(&[
            coin_1.commitment(),
            coin_2.commitment(),
            coin_3.commitment(),
        ]);

        // coin_1 & coin_2 both concurrently win slot 0

        update_ledger(&mut ledger, genesis, 1, coin_1).unwrap();
        let h = update_ledger(&mut ledger, genesis, 1, coin_2).unwrap();

        // then coin_3 wins slot 1 and chooses to extend from block_2
        let h_3 = update_ledger(&mut ledger, h, 2, coin_3).unwrap();
        // coin 1 is not spent in the chain that ends with block_3
        assert!(!ledger.states[&h_3].is_nullified(&coin_1.nullifier()));
    }

    #[test]
    fn test_epoch_transition() {
        let coins = (0..4).map(coin).collect::<Vec<_>>();
        let coin_4 = coin(4);
        let coin_5 = coin(5);
        let (mut ledger, genesis) =
            ledger(&coins.iter().map(|c| c.commitment()).collect::<Vec<_>>());

        // An epoch will be 10 slots long, with stake distribution snapshot taken at the start of the epoch
        // and nonce snapshot before slot 7

        let h_1 = update_ledger(&mut ledger, genesis, 1, coins[0]).unwrap();
        assert_eq!(ledger.states[&h_1].epoch_state.epoch, 0.into());

        let h_2 = update_ledger(&mut ledger, h_1, 6, coins[1]).unwrap();

        let h_3 = apply_and_add_coin(&mut ledger, h_2, 9, coins[2], coin_4);

        // test epoch jump
        let h_4 = update_ledger(&mut ledger, h_3, 20, coins[3]).unwrap();
        // nonce for epoch 2 should be taken at the end of slot 16, but in our case the last block is at slot 9
        assert_eq!(
            ledger.states[&h_4].epoch_state.nonce,
            ledger.states[&h_3].nonce,
        );
        // stake distribution snapshot should be taken at the end of slot 9
        assert_eq!(
            ledger.states[&h_4].epoch_state.commitments,
            ledger.states[&h_3].spend_commitments,
        );

        // nonce for epoch 1 should be taken at the end of slot 6
        let h_5 = apply_and_add_coin(&mut ledger, h_3, 10, coins[3], coin_5);
        assert_eq!(
            ledger.states[&h_5].epoch_state.nonce,
            ledger.states[&h_2].nonce,
        );

        let h_6 = update_ledger(&mut ledger, h_5, 20, coins[3].evolve()).unwrap();
        // stake distribution snapshot should be taken at the end of slot 9, check that changes in slot 10
        // are ignored
        assert_eq!(
            ledger.states[&h_6].epoch_state.commitments,
            ledger.states[&h_3].spend_commitments,
        );
    }

    #[test]
    fn test_evolved_coin_is_eligible_for_leadership() {
        let coin = coin(0);
        let (mut ledger, genesis) = ledger(&[coin.commitment()]);

        let h = update_ledger(&mut ledger, genesis, 1, coin).unwrap();

        // reusing the same coin should be prevented
        assert!(matches!(
            update_ledger(&mut ledger, h, 2, coin),
            Err(LedgerError::NullifierExists),
        ));

        // the evolved coin is not elibile before block 2 as it has not appeared on the ledger yet
        assert!(matches!(
            update_ledger(&mut ledger, genesis, 2, coin.evolve()),
            Err(LedgerError::CommitmentNotFound),
        ));

        // the evolved coin is eligible after coin 1 is spent
        assert!(update_ledger(&mut ledger, h, 2, coin.evolve()).is_ok());
    }

    #[test]
    fn test_new_coins_becoming_eligible_after_stake_distribution_stabilizes() {
        let coin_1 = coin(1);
        let coin = coin(0);

        let (mut ledger, genesis) = ledger(&[coin.commitment()]);

        // EPOCH 0
        // mint a new coin to be used for leader elections in upcoming epochs
        let h_0_1 = apply_and_add_coin(&mut ledger, genesis, 1, coin, coin_1);

        // the new coin is not yet eligible for leader elections
        assert!(matches!(
            update_ledger(&mut ledger, h_0_1, 2, coin_1),
            Err(LedgerError::CommitmentNotFound),
        ));

        // // but the evolved coin can
        let h_0_2 = update_ledger(&mut ledger, h_0_1, 2, coin.evolve()).unwrap();

        // EPOCH 1
        for i in 10..20 {
            // the newly minted coin is still not eligible in the following epoch since the
            // stake distribution snapshot is taken at the beginning of the previous epoch
            assert!(matches!(
                update_ledger(&mut ledger, h_0_2, i, coin_1),
                Err(LedgerError::CommitmentNotFound),
            ));
        }

        // EPOCH 2
        // the coin is finally eligible 2 epochs after it was first minted
        let h_2_0 = update_ledger(&mut ledger, h_0_2, 20, coin_1).unwrap();

        // and now the minted coin can freely use the evolved coin for subsequent blocks
        update_ledger(&mut ledger, h_2_0, 21, coin_1.evolve()).unwrap();
    }

    #[test]
    fn test_orphan_proof_import() {
        let coin = coin(0);
        let (mut ledger, genesis) = ledger(&[coin.commitment()]);

        let coin_new = coin.evolve();
        let coin_new_new = coin_new.evolve();

        // produce a fork where the coin has been spent twice
        let fork_1 = make_id(genesis, 1, coin);
        let fork_2 = make_id(fork_1, 2, coin_new);

        // neither of the evolved coins should be usable right away in another branch
        assert!(matches!(
            update_ledger(&mut ledger, genesis, 1, coin_new),
            Err(LedgerError::CommitmentNotFound)
        ));
        assert!(matches!(
            update_ledger(&mut ledger, genesis, 1, coin_new_new),
            Err(LedgerError::CommitmentNotFound)
        ));

        // they also should not be accepted if the fork from where they have been imported has not been seen already
        assert!(matches!(
            update_orphans(&mut ledger, genesis, 1, coin_new, vec![(fork_1, (1, coin))]),
            Err(LedgerError::OrphanMissing(_))
        ));

        // now the first block of the fork is seen (and accepted)
        let h_1 = update_ledger(&mut ledger, genesis, 1, coin).unwrap();
        assert_eq!(h_1, fork_1);

        // and it can now be imported in another branch (note this does not validate it's for an earlier slot)
        update_orphans(
            &mut ledger.clone(),
            genesis,
            1,
            coin_new,
            vec![(fork_1, (1, coin))],
        )
        .unwrap();
        // but the next coin is still not accepted since the second block using the evolved coin has not been seen yet
        assert!(matches!(
            update_orphans(
                &mut ledger.clone(),
                genesis,
                1,
                coin_new_new,
                vec![(fork_1, (1, coin)), (fork_2, (2, coin_new))],
            ),
            Err(LedgerError::OrphanMissing(_))
        ));

        // now the second block of the fork is seen as well and the coin evolved twice can be used in another branch
        let h_2 = update_ledger(&mut ledger, h_1, 2, coin_new).unwrap();
        assert_eq!(h_2, fork_2);
        update_orphans(
            &mut ledger.clone(),
            genesis,
            1,
            coin_new_new,
            vec![(fork_1, (1, coin)), (fork_2, (2, coin_new))],
        )
        .unwrap();
        // but we can't import just the second proof because it's using an evolved coin that has not been seen yet
        assert!(matches!(
            update_orphans(
                &mut ledger.clone(),
                genesis,
                1,
                coin_new_new,
                vec![(fork_2, (2, coin_new))],
            ),
            Err(LedgerError::CommitmentNotFound)
        ));

        // an imported proof that uses a coin that was already used in the base branch should not be allowed
        let header_1 = update_ledger(&mut ledger, genesis, 1, coin).unwrap();
        assert!(matches!(
            update_orphans(
                &mut ledger,
                header_1,
                2,
                coin_new_new,
                vec![(fork_1, (1, coin)), (fork_2, (2, coin_new))],
            ),
            Err(LedgerError::NullifierExists)
        ));
    }

    #[test]
    fn test_conversions_for_leader_proof() {
        let commitment = Commitment::from([0u8; 32]);
        let commitment_bytes: [u8; 32] = commitment.into();

        let _zero_bytes = [0u8; 32];
        assert!(matches!(commitment_bytes, _zero_bytes));

        let commitment_ref = commitment.as_ref();
        assert_eq!(commitment_ref, &_zero_bytes);

        let nullifier = Nullifier::from([0u8; 32]);
        let _nullifier_bytes: [u8; 32] = nullifier.into();
        assert!(matches!(_nullifier_bytes, _zero_bytes));

        let slot = Slot::genesis();
        let leader_proof = LeaderProof::dummy(slot);

        assert_eq!(leader_proof.commitment(), &commitment);
        assert_eq!(leader_proof.evolved_commitment(), &commitment);
        assert_eq!(leader_proof.nullifier(), &nullifier);

        // Test ser/de of compact representation for Nullifier
        assert_tokens(&nullifier.compact(), &[Token::BorrowedBytes(&[0; 32])]);

        // Test ser/de of compact representation for Commitment
        assert_tokens(&commitment.compact(), &[Token::BorrowedBytes(&[0; 32])]);
    }

    #[test]
    fn test_cryptarchia_ledger_error_cases() {
        let coin = coin(0);
        let commitment = coin.commitment();
        let (ledger, genesis) = ledger(&[commitment]);

        let ledger_state = ledger.state(&genesis).unwrap().clone();
        let ledger_config = ledger.config();

        let slot = Slot::genesis() + 10;
        let ledger_state2 = ledger_state
            .update_epoch_state::<HeaderId>(slot, ledger_config)
            .expect("Ledger needs to move forward");

        let slot2 = Slot::genesis() + 1;
        let update_epoch_err = ledger_state2
            .update_epoch_state::<HeaderId>(slot2, ledger_config)
            .err();

        // Time cannot flow backwards
        match update_epoch_err {
            Some(LedgerError::InvalidSlot { parent, block })
                if parent == slot && block == slot2 => {}
            _ => panic!("Error does not match the LedgerError::InvalidSlot pattern"),
        };
    }
}
