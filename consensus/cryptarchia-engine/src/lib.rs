pub mod block;
pub mod config;
pub mod crypto;
pub mod leader_proof;
pub mod ledger;
pub mod time;

pub use block::*;
pub use config::*;
pub use leader_proof::*;
use ledger::{Ledger, LedgerState};
use std::collections::{HashMap, HashSet};
use thiserror::Error;
pub use time::*;

#[derive(Clone, Debug)]
pub struct Cryptarchia {
    local_chain: Branch,
    branches: Branches,
    ledger: Ledger,
    config: Config,
    genesis: HeaderId,
}

#[derive(Clone, Debug)]
pub struct Branches {
    branches: HashMap<HeaderId, Branch>,
    tips: HashSet<HeaderId>,
}

#[derive(Clone, Debug)]
pub struct Branch {
    header: Header,
    // chain length
    length: u64,
}

impl Branches {
    pub fn from_genesis(genesis: &Header) -> Self {
        let mut branches = HashMap::new();
        branches.insert(
            genesis.id(),
            Branch {
                header: genesis.clone(),
                length: 0,
            },
        );
        let tips = HashSet::from([genesis.id()]);
        Self { branches, tips }
    }

    #[must_use]
    fn apply_header(&self, header: Header) -> Self {
        let mut branches = self.branches.clone();
        let mut tips = self.tips.clone();
        // if the parent was the head of a branch, remove it as it has been superseded by the new header
        tips.remove(&header.parent());
        let length = branches[&header.parent()].length + 1;
        tips.insert(header.id());
        branches.insert(header.id(), Branch { header, length });

        Self { branches, tips }
    }

    pub fn branches(&self) -> Vec<Branch> {
        self.tips
            .iter()
            .map(|id| self.branches[id].clone())
            .collect()
    }

    // find the lowest common ancestor of two branches
    pub fn lca<'a>(&'a self, mut b1: &'a Branch, mut b2: &'a Branch) -> Branch {
        // first reduce branches to the same length
        while b1.length > b2.length {
            b1 = &self.branches[&b1.header.parent()];
        }

        while b2.length > b1.length {
            b2 = &self.branches[&b2.header.parent()];
        }

        // then walk up the chain until we find the common ancestor
        while b1.header.id() != b2.header.id() {
            b1 = &self.branches[&b1.header.parent()];
            b2 = &self.branches[&b2.header.parent()];
        }

        b1.clone()
    }

    pub fn get(&self, id: &HeaderId) -> Option<&Branch> {
        self.branches.get(id)
    }

    // Walk back the chain until the target slot
    fn walk_back_before(&self, branch: &Branch, slot: Slot) -> Branch {
        let mut current = branch;
        while current.header.slot() > slot {
            current = &self.branches[&current.header.parent()];
        }
        current.clone()
    }
}

#[derive(Debug, Clone, Error)]
pub enum Error {
    #[error("Ledger error: {0}")]
    LedgerError(#[from] ledger::LedgerError),
    #[error("Parent block: {0:?} is not know to this node")]
    ParentMissing(HeaderId),
    #[error("Orphan proof has was not found in the ledger: {0:?}, can't import it")]
    OrphanMissing(HeaderId),
}

impl Cryptarchia {
    pub fn from_genesis(header: Header, state: LedgerState, config: Config) -> Self {
        assert_eq!(header.slot(), Slot::genesis());
        Self {
            ledger: Ledger::from_genesis(header.id(), state, config.clone()),
            branches: Branches::from_genesis(&header),
            local_chain: Branch {
                header: header.clone(),
                length: 0,
            },
            config,
            genesis: header.id(),
        }
    }

    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub fn receive_block(&self, block: Block) -> Result<Self, Error> {
        let header = block.header();

        let mut new: Self = self.clone();
        new.branches = new.branches.apply_header(header.clone());
        new.ledger = new.ledger.try_apply_header(header)?;
        new.local_chain = new.fork_choice();

        Ok(new)
    }

    pub fn fork_choice(&self) -> Branch {
        let k = self.config.security_param as u64;
        let s = self.config.s();
        Self::maxvalid_bg(self.local_chain.clone(), &self.branches, k, s)
    }

    pub fn tip(&self) -> &Header {
        &self.local_chain.header
    }

    pub fn tip_id(&self) -> HeaderId {
        self.local_chain.header.id()
    }

    // prune all states deeper than 'depth' with regard to the current
    // local chain except for states belonging to the local chain
    pub fn prune_forks(&mut self, _depth: u64) {
        todo!()
    }

    pub fn genesis(&self) -> &HeaderId {
        &self.genesis
    }

    pub fn branches(&self) -> &Branches {
        &self.branches
    }

    //  Implementation of the fork choice rule as defined in the Ouroboros Genesis paper
    //  k defines the forking depth of chain we accept without more analysis
    //  s defines the length of time (unit of slots) after the fork happened we will inspect for chain density
    fn maxvalid_bg(local_chain: Branch, branches: &Branches, k: u64, s: u64) -> Branch {
        let mut cmax = local_chain;
        let forks = branches.branches();
        for chain in forks {
            let lowest_common_ancestor = branches.lca(&cmax, &chain);
            let m = cmax.length - lowest_common_ancestor.length;
            if m <= k {
                // Classic longest chain rule with parameter k
                if cmax.length < chain.length {
                    cmax = chain;
                }
            } else {
                // The chain is forking too much, we need to pay a bit more attention
                // In particular, select the chain that is the densest after the fork
                let density_slot = Slot::from(u64::from(lowest_common_ancestor.header.slot()) + s);
                let cmax_density = branches.walk_back_before(&cmax, density_slot).length;
                let candidate_density = branches.walk_back_before(&chain, density_slot).length;
                if cmax_density < candidate_density {
                    cmax = chain;
                }
            }
        }
        cmax
    }
}

#[cfg(test)]
pub mod tests {
    use crate::{
        crypto::Blake2b, Block, Commitment, Config, Header, HeaderId, LeaderProof, Nullifier, Slot,
        TimeConfig,
    };
    use blake2::Digest;
    use std::hash::{DefaultHasher, Hash, Hasher};

    use super::{
        ledger::{tests::genesis_state, LedgerError},
        Cryptarchia, Error,
    };

    pub fn header(slot: impl Into<Slot>, parent: HeaderId, coin: Coin) -> Header {
        let slot = slot.into();
        Header::new(parent, 0, [0; 32].into(), slot, coin.to_proof(slot))
    }

    pub fn block(slot: impl Into<Slot>, parent: HeaderId, coin: Coin) -> Block {
        Block::new(header(slot, parent, coin))
    }

    pub fn block_with_orphans(
        slot: impl Into<Slot>,
        parent: HeaderId,
        coin: Coin,
        orphans: Vec<Header>,
    ) -> Block {
        Block::new(header(slot, parent, coin).with_orphaned_proofs(orphans))
    }

    pub fn propose_and_evolve(
        slot: impl Into<Slot>,
        parent: HeaderId,
        coin: &mut Coin,
        eng: &mut Cryptarchia,
    ) -> HeaderId {
        let b = block(slot, parent, *coin);
        let id = b.header().id();
        *eng = eng.receive_block(b).unwrap();
        *coin = coin.evolve();
        id
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

    fn engine(commitments: &[Commitment]) -> Cryptarchia {
        Cryptarchia::from_genesis(genesis_header(), genesis_state(commitments), config())
    }

    pub fn config() -> Config {
        Config {
            security_param: 1,
            active_slot_coeff: 1.0,
            epoch_stake_distribution_stabilization: 4,
            epoch_period_nonce_buffer: 3,
            epoch_period_nonce_stabilization: 3,
            time: TimeConfig {
                slot_duration: 1,
                chain_start_time: 0,
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

    #[test]
    fn test_fork_choice() {
        let mut long_coin = Coin::new(0);
        let mut short_coin = Coin::new(1);
        let mut long_dense_coin = Coin::new(2);
        // TODO: use cryptarchia
        let mut engine = engine(&[
            long_coin.commitment(),
            short_coin.commitment(),
            long_dense_coin.commitment(),
        ]);
        // by setting a low k we trigger the density choice rule, and the shorter chain is denser after
        // the fork
        engine.config.security_param = 10;

        let mut parent = *engine.genesis();
        for i in 1..50 {
            parent = propose_and_evolve(i, parent, &mut long_coin, &mut engine);
            println!("{:?}", engine.tip());
        }
        println!("{:?}", engine.tip());
        assert_eq!(engine.tip_id(), parent);

        let mut long_p = parent;
        let mut short_p = parent;
        // the node sees first the short chain
        for slot in 50..70 {
            short_p = propose_and_evolve(slot, short_p, &mut short_coin, &mut engine);
        }

        assert_eq!(engine.tip_id(), short_p);

        // then it receives a longer chain which is however less dense after the fork
        for slot in 50..70 {
            if slot % 2 == 0 {
                long_p = propose_and_evolve(slot, long_p, &mut long_coin, &mut engine);
            }
            assert_eq!(engine.tip_id(), short_p);
        }
        // even if the long chain is much longer, it will never be accepted as it's not dense enough
        for slot in 70..100 {
            long_p = propose_and_evolve(slot, long_p, &mut long_coin, &mut engine);
            assert_eq!(engine.tip_id(), short_p);
        }

        let bs = engine.branches().branches();
        let long_branch = bs.iter().find(|b| b.header.id() == long_p).unwrap();
        let short_branch = bs.iter().find(|b| b.header.id() == short_p).unwrap();
        assert!(long_branch.length > short_branch.length);

        // however, if we set k to the fork length, it will be accepted
        let k = long_branch.length;
        assert_eq!(
            Cryptarchia::maxvalid_bg(
                short_branch.clone(),
                engine.branches(),
                k,
                engine.config.s()
            )
            .header
            .id(),
            long_p
        );

        // a longer chain which is equally dense after the fork will be selected as the main tip
        for slot in 50..71 {
            parent = propose_and_evolve(slot, parent, &mut long_dense_coin, &mut engine);
        }
        assert_eq!(engine.tip_id(), parent);
    }

    #[test]
    fn test_orphan_proof_import() {
        let coin = Coin::new(0);
        let mut engine = engine(&[coin.commitment()]);

        let coin_new = coin.evolve();
        let coin_new_new = coin_new.evolve();

        // produce a fork where the coin has been spent twice
        let fork_1 = block(1, *engine.genesis(), coin);
        let fork_2 = block(2, fork_1.header().id(), coin_new);

        // neither of the evolved coins should be usable right away in another branch
        assert!(matches!(
            engine.receive_block(block(1, *engine.genesis(), coin_new)),
            Err(Error::LedgerError(LedgerError::CommitmentNotFound))
        ));
        assert!(matches!(
            engine.receive_block(block(1, *engine.genesis(), coin_new_new)),
            Err(Error::LedgerError(LedgerError::CommitmentNotFound))
        ));

        // they also should not be accepted if the fork from where they have been imported has not been seen already
        assert!(matches!(
            engine.receive_block(block_with_orphans(
                1,
                *engine.genesis(),
                coin_new,
                vec![fork_1.header().clone()]
            )),
            Err(Error::LedgerError(LedgerError::OrphanMissing(_)))
        ));

        // now the first block of the fork is seen (and accepted)
        engine = engine.receive_block(fork_1.clone()).unwrap();
        // and it can now be imported in another branch (note this does not validate it's for an earlier slot)
        engine
            .receive_block(block_with_orphans(
                1,
                *engine.genesis(),
                coin_new,
                vec![fork_1.header().clone()],
            ))
            .unwrap();
        // but the next coin is still not accepted since the second block using the evolved coin has not been seen yet
        assert!(matches!(
            engine.receive_block(block_with_orphans(
                1,
                *engine.genesis(),
                coin_new_new,
                vec![fork_1.header().clone(), fork_2.header().clone()]
            )),
            Err(Error::LedgerError(LedgerError::OrphanMissing(_)))
        ));

        // now the second block of the fork is seen as well and the coin evolved twice can be used in another branch
        engine = engine.receive_block(fork_2.clone()).unwrap();
        engine
            .receive_block(block_with_orphans(
                1,
                *engine.genesis(),
                coin_new_new,
                vec![fork_1.header().clone(), fork_2.header().clone()],
            ))
            .unwrap();
        // but we can't import just the second proof because it's using an evolved coin that has not been seen yet
        assert!(matches!(
            engine.receive_block(block_with_orphans(
                1,
                *engine.genesis(),
                coin_new_new,
                vec![fork_2.header().clone()]
            )),
            Err(Error::LedgerError(LedgerError::CommitmentNotFound))
        ));

        // an imported proof that uses a coin that was already used in the base branch should not be allowed
        let block_1 = block(1, *engine.genesis(), coin);
        engine = engine.receive_block(block_1.clone()).unwrap();
        assert!(matches!(
            engine.receive_block(block_with_orphans(
                2,
                block_1.header().id(),
                coin_new_new,
                vec![fork_1.header().clone(), fork_2.header().clone()]
            )),
            Err(Error::LedgerError(LedgerError::NullifierExists))
        ));
    }
}
