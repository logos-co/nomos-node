pub mod config;

mod time;

pub use config::*;
use std::collections::{HashMap, HashSet};
use thiserror::Error;
pub use time::{Epoch, Slot};

#[derive(Clone, Debug)]
pub struct Cryptarchia<Id> {
    local_chain: Branch<Id>,
    branches: Branches<Id>,
    config: Config,
    genesis: Id,
}

#[derive(Clone, Debug)]
pub struct Branches<Id> {
    branches: HashMap<Id, Branch<Id>>,
    tips: HashSet<Id>,
}

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Branch<Id> {
    id: Id,
    parent: Id,
    slot: Slot,
    // chain length
    length: u64,
}

impl<Id: Copy> Branch<Id> {
    pub fn id(&self) -> Id {
        self.id
    }
    pub fn parent(&self) -> Id {
        self.parent
    }
    pub fn slot(&self) -> Slot {
        self.slot
    }
    pub fn length(&self) -> u64 {
        self.length
    }
}

impl<Id> Branches<Id>
where
    Id: Eq + std::hash::Hash + Copy,
{
    pub fn from_genesis(genesis: Id) -> Self {
        let mut branches = HashMap::new();
        branches.insert(
            genesis,
            Branch {
                id: genesis,
                parent: genesis,
                slot: 0.into(),
                length: 0,
            },
        );
        let tips = HashSet::from([genesis]);
        Self { branches, tips }
    }

    #[must_use = "this returns the result of the operation, without modifying the original"]
    fn apply_header(&self, header: Id, parent: Id, slot: Slot) -> Result<Self, Error<Id>> {
        let mut branches = self.branches.clone();
        let mut tips = self.tips.clone();
        // if the parent was the head of a branch, remove it as it has been superseded by the new header
        tips.remove(&parent);
        let length = branches
            .get(&parent)
            .ok_or(Error::ParentMissing(parent))?
            .length
            + 1;
        tips.insert(header);
        branches.insert(
            header,
            Branch {
                id: header,
                parent,
                length,
                slot,
            },
        );

        Ok(Self { branches, tips })
    }

    pub fn branches(&self) -> Vec<Branch<Id>> {
        self.tips
            .iter()
            .map(|id| self.branches[id].clone())
            .collect()
    }

    // find the lowest common ancestor of two branches
    pub fn lca<'a>(&'a self, mut b1: &'a Branch<Id>, mut b2: &'a Branch<Id>) -> Branch<Id> {
        // first reduce branches to the same length
        while b1.length > b2.length {
            b1 = &self.branches[&b1.parent];
        }

        while b2.length > b1.length {
            b2 = &self.branches[&b2.parent];
        }

        // then walk up the chain until we find the common ancestor
        while b1.id != b2.id {
            b1 = &self.branches[&b1.parent];
            b2 = &self.branches[&b2.parent];
        }

        b1.clone()
    }

    pub fn get(&self, id: &Id) -> Option<&Branch<Id>> {
        self.branches.get(id)
    }

    // Walk back the chain until the target slot
    fn walk_back_before(&self, branch: &Branch<Id>, slot: Slot) -> Branch<Id> {
        let mut current = branch;
        while current.slot > slot {
            current = &self.branches[&current.parent];
        }
        current.clone()
    }
}

#[derive(Debug, Clone, Error)]
#[cfg_attr(test, derive(PartialEq))]
pub enum Error<Id> {
    #[error("Parent block: {0:?} is not know to this node")]
    ParentMissing(Id),
    #[error("Orphan proof has was not found in the ledger: {0:?}, can't import it")]
    OrphanMissing(Id),
}

impl<Id> Cryptarchia<Id>
where
    Id: Eq + std::hash::Hash + Copy,
{
    pub fn from_genesis(id: Id, config: Config) -> Self {
        Self {
            branches: Branches::from_genesis(id),
            local_chain: Branch {
                id,
                length: 0,
                parent: id,
                slot: 0.into(),
            },
            config,
            genesis: id,
        }
    }

    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub fn receive_block(&self, id: Id, parent: Id, slot: Slot) -> Result<Self, Error<Id>> {
        let mut new: Self = self.clone();
        new.branches = new.branches.apply_header(id, parent, slot)?;
        new.local_chain = new.fork_choice();

        Ok(new)
    }

    pub fn fork_choice(&self) -> Branch<Id> {
        let k = self.config.security_param as u64;
        let s = self.config.s();
        Self::maxvalid_bg(self.local_chain.clone(), &self.branches, k, s)
    }

    pub fn tip(&self) -> Id {
        self.local_chain.id
    }

    // prune all states deeper than 'depth' with regard to the current
    // local chain except for states belonging to the local chain
    pub fn prune_forks(&mut self, _depth: u64) {
        todo!()
    }

    pub fn genesis(&self) -> Id {
        self.genesis
    }

    pub fn branches(&self) -> &Branches<Id> {
        &self.branches
    }

    //  Implementation of the fork choice rule as defined in the Ouroboros Genesis paper
    //  k defines the forking depth of chain we accept without more analysis
    //  s defines the length of time (unit of slots) after the fork happened we will inspect for chain density
    fn maxvalid_bg(local_chain: Branch<Id>, branches: &Branches<Id>, k: u64, s: u64) -> Branch<Id> {
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
                let density_slot = Slot::from(u64::from(lowest_common_ancestor.slot) + s);
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
    use super::{Cryptarchia, Error, Slot};
    use crate::Config;
    use std::hash::{DefaultHasher, Hash, Hasher};

    pub fn config() -> Config {
        Config {
            security_param: 1,
            active_slot_coeff: 1.0,
        }
    }

    #[test]
    fn test_fork_choice() {
        // TODO: use cryptarchia
        let mut engine = Cryptarchia::from_genesis([0; 32], config());
        // by setting a low k we trigger the density choice rule, and the shorter chain is denser after
        // the fork
        engine.config.security_param = 10;

        let mut parent = engine.genesis();
        for i in 1..50 {
            let new_block = hash(&i);
            engine = engine.receive_block(new_block, parent, i.into()).unwrap();
            parent = new_block;
            println!("{:?}", engine.tip());
        }
        println!("{:?}", engine.tip());
        assert_eq!(engine.tip(), parent);

        let mut long_p = parent;
        let mut short_p = parent;
        // the node sees first the short chain
        for slot in 50..70 {
            let new_block = hash(&format!("short-{}", slot));
            engine = engine
                .receive_block(new_block, short_p, slot.into())
                .unwrap();
            short_p = new_block;
        }

        assert_eq!(engine.tip(), short_p);

        // then it receives a longer chain which is however less dense after the fork
        for slot in 50..70 {
            if slot % 2 == 0 {
                let new_block = hash(&format!("long-{}", slot));
                engine = engine
                    .receive_block(new_block, long_p, slot.into())
                    .unwrap();
                long_p = new_block;
            }
            assert_eq!(engine.tip(), short_p);
        }
        // even if the long chain is much longer, it will never be accepted as it's not dense enough
        for slot in 70..100 {
            let new_block = hash(&format!("long-{}", slot));
            engine = engine
                .receive_block(new_block, long_p, slot.into())
                .unwrap();
            long_p = new_block;
            assert_eq!(engine.tip(), short_p);
        }

        let bs = engine.branches().branches();
        let long_branch = bs.iter().find(|b| b.id == long_p).unwrap();
        let short_branch = bs.iter().find(|b| b.id == short_p).unwrap();
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
            .id,
            long_p
        );

        // a longer chain which is equally dense after the fork will be selected as the main tip
        for slot in 50..71 {
            let new_block = hash(&format!("long-dense-{}", slot));
            engine = engine
                .receive_block(new_block, parent, slot.into())
                .unwrap();
            parent = new_block;
        }
        assert_eq!(engine.tip(), parent);
    }

    fn hash<T: Hash>(t: &T) -> [u8; 32] {
        let mut s = DefaultHasher::new();
        t.hash(&mut s);
        let hash = s.finish();
        let mut res = [0; 32];
        res[..8].copy_from_slice(&hash.to_be_bytes());
        res
    }

    #[test]
    fn test_getters() {
        let engine = Cryptarchia::from_genesis([0; 32], config());
        let parent = engine.genesis();

        // Get branch directly from HashMap
        let branch1 = engine
            .branches
            .get(&parent)
            .ok_or("At least one branch should be there");

        let branches = engine.branches();

        // Get branch using getter
        let branch2 = branches
            .get(&parent)
            .ok_or("At least one branch should be there");

        assert_eq!(branch1, branch2);
        assert_eq!(
            branch1.expect("id is not set").id(),
            branch2.expect("id is not set").id()
        );
        assert_eq!(
            branch1.expect("parent is not set").parent(),
            branch2.expect("parent is not set").parent()
        );
        assert_eq!(
            branch1.expect("slot is not set").slot(),
            branch2.expect("slot is not set").slot()
        );
        assert_eq!(
            branch1.expect("length is not set").length(),
            branch2.expect("length is not set").length()
        );

        let slot = Slot::genesis();

        assert_eq!(slot + 10u64, Slot::from(10));

        let strange_engine = Cryptarchia::from_genesis([100; 32], config());
        let not_a_parent = strange_engine.genesis();

        _ = match branches
            .get(&not_a_parent)
            .ok_or(Error::ParentMissing(not_a_parent))
        {
            Ok(_) => panic!("Parent should not be related to this branch"),
            Err(e) => {
                assert_ne!(e, Error::ParentMissing(parent));
            }
        };
    }
}
