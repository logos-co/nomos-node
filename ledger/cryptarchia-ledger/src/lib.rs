mod block;
mod config;
mod crypto;
mod leader_proof;

use crate::{crypto::Blake2b, Commitment, LeaderProof, Nullifier};
use blake2::Digest;
use cryptarchia_engine::{Epoch, Slot};
use rpds::HashTrieSet;
use std::collections::HashMap;
use thiserror::Error;

pub use block::*;
pub use config::Config;
pub use leader_proof::*;

#[derive(Clone, Debug, Error)]
pub enum LedgerError {
    #[error("Commitment not found in the ledger state")]
    CommitmentNotFound,
    #[error("Nullifier already exists in the ledger state")]
    NullifierExists,
    #[error("Commitment already exists in the ledger state")]
    CommitmentExists,
    #[error("Invalid block slot {block:?} for parent slot {parent:?}")]
    InvalidSlot { parent: Slot, block: Slot },
    #[error("Parent block not found: {0:?}")]
    ParentNotFound(HeaderId),
    #[error("Orphan block missing: {0:?}. Importing leader proofs requires the block to be validated first")]
    OrphanMissing(HeaderId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EpochState {
    // The epoch this snapshot is for
    epoch: Epoch,
    // value of the ledger nonce after 'epoch_period_nonce_buffer' slots from the beginning of the epoch
    nonce: Nonce,
    // stake distribution snapshot taken at the beginning of the epoch
    // (in practice, this is equivalent to the coins the are spendable at the beginning of the epoch)
    commitments: HashTrieSet<Commitment>,
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
        }
    }

    fn is_eligible_leader(&self, commitment: &Commitment) -> bool {
        self.commitments.contains(commitment)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ledger {
    states: HashMap<HeaderId, LedgerState>,
    config: Config,
}

impl Ledger {
    pub fn from_genesis(id: HeaderId, state: LedgerState, config: Config) -> Self {
        Self {
            states: [(id, state)].into_iter().collect(),
            config,
        }
    }

    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub fn try_apply_header(&self, header: &Header) -> Result<Self, LedgerError> {
        let parent_id = header.parent();
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
        for orphan in header.orphaned_proofs() {
            if !self.states.contains_key(&orphan.id()) {
                return Err(LedgerError::OrphanMissing(orphan.id()));
            }
        }

        let new_state = parent_state
            .clone()
            .try_apply_header(header, &self.config)?;

        let mut states = self.states.clone();

        states.insert(header.id(), new_state);

        Ok(Self { states, config })
    }

    pub fn state(&self, header_id: &HeaderId) -> Option<&LedgerState> {
        self.states.get(header_id)
    }
}

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
    fn try_apply_header(self, header: &Header, config: &Config) -> Result<Self, LedgerError> {
        // TODO: import leader proofs
        self.update_epoch_state(header.slot(), config)?
            .try_apply_leadership(header, config)
    }

    fn update_epoch_state(self, slot: Slot, config: &Config) -> Result<Self, LedgerError> {
        if slot <= self.slot {
            return Err(LedgerError::InvalidSlot {
                parent: self.slot,
                block: slot,
            });
        }

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
            };
            let next_epoch_state = EpochState {
                epoch: new_epoch + 1,
                nonce: self.nonce,
                commitments: self.spend_commitments.clone(),
            };
            Ok(Self {
                slot,
                next_epoch_state,
                epoch_state,
                ..self
            })
        }
    }

    fn try_apply_proof(self, proof: &LeaderProof, config: &Config) -> Result<Self, LedgerError> {
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

    fn try_apply_leadership(
        mut self,
        header: &Header,
        config: &Config,
    ) -> Result<Self, LedgerError> {
        for proof in header.orphaned_proofs() {
            self = self.try_apply_proof(proof.leader_proof(), config)?;
        }

        self = self
            .try_apply_proof(header.leader_proof(), config)?
            .update_nonce(header.leader_proof());

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
    use super::{EpochState, Ledger, LedgerState};
    use crate::{
        crypto::Blake2b, Commitment, Config, Header, HeaderId, LeaderProof, LedgerError, Nullifier,
    };
    use blake2::Digest;
    use cryptarchia_engine::Slot;
    use std::hash::{DefaultHasher, Hash, Hasher};

    pub fn header(slot: impl Into<Slot>, parent: HeaderId, coin: Coin) -> Header {
        let slot = slot.into();
        Header::new(parent, 0, [0; 32].into(), slot, coin.to_proof(slot))
    }

    pub fn header_with_orphans(
        slot: impl Into<Slot>,
        parent: HeaderId,
        coin: Coin,
        orphans: Vec<Header>,
    ) -> Header {
        header(slot, parent, coin).with_orphaned_proofs(orphans)
    }

    pub fn genesis_header() -> Header {
        Header::new(
            [0; 32].into(),
            0,
            [0; 32].into(),
            0.into(),
            LeaderProof::dummy(0.into()),
        )
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

    #[derive(Debug, Clone, Copy)]
    pub struct Coin {
        sk: u64,
        nonce: u64,
    }

    impl Coin {
        pub fn new(sk: u64) -> Self {
            Self { sk, nonce: 0 }
        }

        pub fn commitment(&self) -> Commitment {
            <[u8; 32]>::from(
                Blake2b::new_with_prefix("commitment".as_bytes())
                    .chain_update(self.sk.to_be_bytes())
                    .chain_update(self.nonce.to_be_bytes())
                    .finalize(),
            )
            .into()
        }

        pub fn nullifier(&self) -> Nullifier {
            <[u8; 32]>::from(
                Blake2b::new_with_prefix("nullifier".as_bytes())
                    .chain_update(self.sk.to_be_bytes())
                    .chain_update(self.nonce.to_be_bytes())
                    .finalize(),
            )
            .into()
        }

        pub fn evolve(&self) -> Self {
            let mut h = DefaultHasher::new();
            self.nonce.hash(&mut h);
            let nonce = h.finish();
            Self { sk: self.sk, nonce }
        }

        pub fn to_proof(&self, slot: Slot) -> LeaderProof {
            LeaderProof::new(
                self.commitment(),
                self.nullifier(),
                slot,
                self.evolve().commitment(),
            )
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
            },
            epoch_state: EpochState {
                epoch: 0.into(),
                nonce: [0; 32].into(),
                commitments: commitments.iter().cloned().collect(),
            },
        }
    }

    fn ledger(commitments: &[Commitment]) -> (Ledger, Header) {
        let genesis_state = genesis_state(commitments);
        let genesis_header = genesis_header();
        (
            Ledger::from_genesis(genesis_header.id(), genesis_state, config()),
            genesis_header,
        )
    }

    fn apply_and_add_coin(mut ledger: Ledger, header: Header, coin: Coin) -> Ledger {
        let header_id = header.id();
        ledger = ledger.try_apply_header(&header).unwrap();
        // we still don't have transactions, so the only way to add a commitment to spendable commitments and
        // test epoch snapshotting is by doing this manually
        let mut block_state = ledger.states[&header_id].clone();
        block_state.spend_commitments = block_state.spend_commitments.insert(coin.commitment());
        ledger.states.insert(header_id, block_state);
        ledger
    }

    #[test]
    fn test_ledger_state_prevents_coin_reuse() {
        let coin = Coin::new(0);
        let (mut ledger, genesis) = ledger(&[coin.commitment()]);
        let h = header(1, genesis.id(), coin);
        ledger = ledger.try_apply_header(&h).unwrap();

        // reusing the same coin should be prevented
        assert!(matches!(
            ledger.try_apply_header(&header(2, h.id(), coin)),
            Err(LedgerError::NullifierExists),
        ));
    }

    #[test]
    fn test_ledger_state_uncommited_coin() {
        let coin = Coin::new(0);
        let (ledger, genesis) = ledger(&[]);
        let h = header(1, genesis.id(), coin);
        assert!(matches!(
            ledger.try_apply_header(&h),
            Err(LedgerError::CommitmentNotFound),
        ));
    }

    #[test]
    fn test_ledger_state_is_properly_updated_on_reorg() {
        let coin_1 = Coin::new(0);
        let coin_2 = Coin::new(1);
        let coin_3 = Coin::new(2);

        let (mut ledger, genesis) = ledger(&[
            coin_1.commitment(),
            coin_2.commitment(),
            coin_3.commitment(),
        ]);

        // coin_1 & coin_2 both concurrently win slot 0
        let h_1 = header(1, genesis.id(), coin_1);
        let h_2 = header(1, genesis.id(), coin_2);

        ledger = ledger.try_apply_header(&h_1).unwrap();
        ledger = ledger.try_apply_header(&h_2).unwrap();

        // then coin_3 wins slot 1 and chooses to extend from block_2
        let h_3 = header(2, h_2.id(), coin_3);
        ledger = ledger.try_apply_header(&h_3).unwrap();
        // coin 1 is not spent in the chain that ends with block_3
        assert!(!ledger.states[&h_3.id()].is_nullified(&coin_1.nullifier()));
    }

    #[test]
    fn test_epoch_transition() {
        let coins = (0..4).map(Coin::new).collect::<Vec<_>>();
        let coin_4 = Coin::new(4);
        let coin_5 = Coin::new(5);
        let (mut ledger, genesis) =
            ledger(&coins.iter().map(|c| c.commitment()).collect::<Vec<_>>());

        // An epoch will be 10 slots long, with stake distribution snapshot taken at the start of the epoch
        // and nonce snapshot before slot 7

        let h_1 = header(1, genesis.id(), coins[0]);
        ledger = ledger.try_apply_header(&h_1).unwrap();
        assert_eq!(ledger.states[&h_1.id()].epoch_state.epoch, 0.into());

        let h_2 = header(6, h_1.id(), coins[1]);
        ledger = ledger.try_apply_header(&h_2).unwrap();

        let h_3 = header(9, h_2.id(), coins[2]);
        ledger = apply_and_add_coin(ledger, h_3.clone(), coin_4);

        // test epoch jump
        let h_4 = header(20, h_3.id(), coins[3]);
        ledger = ledger.try_apply_header(&h_4).unwrap();
        // nonce for epoch 2 should be taken at the end of slot 16, but in our case the last block is at slot 9
        assert_eq!(
            ledger.states[&h_4.id()].epoch_state.nonce,
            ledger.states[&h_3.id()].nonce,
        );
        // stake distribution snapshot should be taken at the end of slot 9
        assert_eq!(
            ledger.states[&h_4.id()].epoch_state.commitments,
            ledger.states[&h_3.id()].spend_commitments,
        );

        // nonce for epoch 1 should be taken at the end of slot 6
        let h_5 = header(10, h_3.id(), coins[3]);
        ledger = apply_and_add_coin(ledger, h_5.clone(), coin_5);
        assert_eq!(
            ledger.states[&h_5.id()].epoch_state.nonce,
            ledger.states[&h_2.id()].nonce,
        );

        let h_6 = header(20, h_5.id(), coins[3].evolve());
        ledger = ledger.try_apply_header(&h_6).unwrap();
        // stake distribution snapshot should be taken at the end of slot 9, check that changes in slot 10
        // are ignored
        assert_eq!(
            ledger.states[&h_6.id()].epoch_state.commitments,
            ledger.states[&h_3.id()].spend_commitments,
        );
    }

    #[test]
    fn test_evolved_coin_is_eligible_for_leadership() {
        let coin = Coin::new(0);
        let (mut ledger, genesis) = ledger(&[coin.commitment()]);
        let h = header(1, genesis.id(), coin);
        ledger = ledger.try_apply_header(&h).unwrap();

        // reusing the same coin should be prevented
        assert!(matches!(
            ledger.try_apply_header(&header(2, h.id(), coin)),
            Err(LedgerError::NullifierExists),
        ));

        // the evolved coin is not elibile before block 2 as it has not appeared on the ledger yet
        assert!(matches!(
            ledger.try_apply_header(&header(2, genesis.id(), coin.evolve())),
            Err(LedgerError::CommitmentNotFound),
        ));

        // the evolved coin is eligible after coin 1 is spent
        assert!(ledger
            .try_apply_header(&header(2, h.id(), coin.evolve()))
            .is_ok());
    }

    #[test]
    fn test_new_coins_becoming_eligible_after_stake_distribution_stabilizes() {
        let coin = Coin::new(0);
        let coin_1 = Coin::new(1);
        let (mut ledger, genesis) = ledger(&[coin.commitment()]);

        // EPOCH 0
        let h_0_1 = header(1, genesis.id(), coin);
        // mint a new coin to be used for leader elections in upcoming epochs
        ledger = apply_and_add_coin(ledger, h_0_1.clone(), coin_1);

        let h_0_2 = header(2, h_0_1.id(), coin_1);
        // the new coin is not yet eligible for leader elections
        assert!(matches!(
            ledger.try_apply_header(&h_0_2),
            Err(LedgerError::CommitmentNotFound),
        ));

        // but the evolved coin can
        let h_0_2 = header(2, h_0_1.id(), coin.evolve());
        ledger = ledger.try_apply_header(&h_0_2).unwrap();

        // EPOCH 1
        for i in 10..20 {
            // the newly minted coin is still not eligible in the following epoch since the
            // stake distribution snapshot is taken at the beginning of the previous epoch
            assert!(matches!(
                ledger.try_apply_header(&header(i, h_0_2.id(), coin_1)),
                Err(LedgerError::CommitmentNotFound),
            ));
        }

        // EPOCH 2
        // the coin is finally eligible 2 epochs after it was first minted
        let h_2_0 = header(20, h_0_2.id(), coin_1);
        ledger = ledger.try_apply_header(&h_2_0).unwrap();

        // and now the minted coin can freely use the evolved coin for subsequent blocks
        let h_2_1 = header(21, h_2_0.id(), coin_1.evolve());
        ledger.try_apply_header(&h_2_1).unwrap();
    }

    #[test]
    fn test_orphan_proof_import() {
        let coin = Coin::new(0);
        let (mut ledger, genesis) = ledger(&[coin.commitment()]);

        let coin_new = coin.evolve();
        let coin_new_new = coin_new.evolve();

        // produce a fork where the coin has been spent twice
        let fork_1 = header(1, genesis.id(), coin);
        let fork_2 = header(2, fork_1.id(), coin_new);

        // neither of the evolved coins should be usable right away in another branch
        assert!(matches!(
            ledger.try_apply_header(&header(1, genesis.id(), coin_new)),
            Err(LedgerError::CommitmentNotFound)
        ));
        assert!(matches!(
            ledger.try_apply_header(&header(1, genesis.id(), coin_new_new)),
            Err(LedgerError::CommitmentNotFound)
        ));

        // they also should not be accepted if the fork from where they have been imported has not been seen already
        assert!(matches!(
            ledger.try_apply_header(&header_with_orphans(
                1,
                genesis.id(),
                coin_new,
                vec![fork_1.clone()]
            )),
            Err(LedgerError::OrphanMissing(_))
        ));

        // now the first block of the fork is seen (and accepted)
        ledger = ledger.try_apply_header(&fork_1).unwrap();
        // and it can now be imported in another branch (note this does not validate it's for an earlier slot)
        ledger
            .try_apply_header(&header_with_orphans(
                1,
                genesis.id(),
                coin_new,
                vec![fork_1.clone()],
            ))
            .unwrap();
        // but the next coin is still not accepted since the second block using the evolved coin has not been seen yet
        assert!(matches!(
            ledger.try_apply_header(&header_with_orphans(
                1,
                genesis.id(),
                coin_new_new,
                vec![fork_1.clone(), fork_2.clone()]
            )),
            Err(LedgerError::OrphanMissing(_))
        ));

        // now the second block of the fork is seen as well and the coin evolved twice can be used in another branch
        ledger = ledger.try_apply_header(&fork_2).unwrap();
        ledger
            .try_apply_header(&header_with_orphans(
                1,
                genesis.id(),
                coin_new_new,
                vec![fork_1.clone(), fork_2.clone()],
            ))
            .unwrap();
        // but we can't import just the second proof because it's using an evolved coin that has not been seen yet
        assert!(matches!(
            ledger.try_apply_header(&header_with_orphans(
                1,
                genesis.id(),
                coin_new_new,
                vec![fork_2.clone()]
            )),
            Err(LedgerError::CommitmentNotFound)
        ));

        // an imported proof that uses a coin that was already used in the base branch should not be allowed
        let header_1 = header(1, genesis.id(), coin);
        ledger = ledger.try_apply_header(&header_1).unwrap();
        assert!(matches!(
            ledger.try_apply_header(&header_with_orphans(
                2,
                header_1.id(),
                coin_new_new,
                vec![fork_1.clone(), fork_2.clone()]
            )),
            Err(LedgerError::NullifierExists)
        ));
    }
}
