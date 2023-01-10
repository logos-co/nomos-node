// std
use std::collections::BTreeMap;
// crates
// internal
use crate::backend::MemPool;
use nomos_core::block::{BlockHeader, BlockId};

/// A mock mempool implementation that stores all transactions in memory in the order received.
pub struct MockPool<Id, Tx> {
    pending_txs_lookup: BTreeMap<Id, usize>,
    pending_txs_by_index: Vec<Tx>,
    in_block_txs: BTreeMap<BlockId, Vec<Tx>>,
    in_block_txs_lookup: BTreeMap<Id, BlockId>,
}

impl<Id, Tx> Default for MockPool<Id, Tx> {
    fn default() -> Self {
        Self {
            pending_txs_lookup: BTreeMap::new(),
            pending_txs_by_index: Vec::new(),
            in_block_txs: BTreeMap::new(),
            in_block_txs_lookup: BTreeMap::new(),
        }
    }
}

impl<Id, Tx> MockPool<Id, Tx> {
    pub fn new() -> Self {
        Default::default()
    }
}

impl<Id, Tx> MemPool for MockPool<Id, Tx>
where
    Id: From<Tx> + PartialOrd + Ord,
    Tx: Clone + Send + Sync + 'static,
{
    type Settings = ();
    type Tx = Tx;
    type Id = Id;

    fn new(_settings: Self::Settings) -> Self {
        Self::new()
    }

    fn add_tx(&mut self, tx: Self::Tx) -> Result<(), overwatch_rs::DynError> {
        let id = Id::from(tx.clone());
        if self.pending_txs_lookup.contains_key(&id) || self.in_block_txs_lookup.contains_key(&id) {
            return Ok(());
        }
        let index = self.pending_txs_by_index.len();
        self.pending_txs_lookup.insert(id, index);
        self.pending_txs_by_index.push(tx);
        Ok(())
    }

    fn view(&self, _ancestor_hint: BlockId) -> Box<dyn Iterator<Item = Self::Tx> + Send> {
        // we need to have an owned version of the iterator to bypass adding a lifetime bound to the return iterator type
        #[allow(clippy::unnecessary_to_owned)]
        Box::new(self.pending_txs_by_index.to_vec().into_iter())
    }

    fn mark_in_block(&mut self, txs: &[Self::Id], block: BlockHeader) {
        let mut txs_in_block = Vec::new();
        for tx_id in txs.iter() {
            if let Some(index) = self.pending_txs_lookup.remove(tx_id) {
                let tx = self.pending_txs_by_index.remove(index);
                txs_in_block.push(tx);
            }
        }
        self.in_block_txs.insert(block.id(), txs_in_block);
    }

    fn prune(&mut self, txs: &[Self::Id]) {
        for tx_id in txs {
            if let Some(i) = self.pending_txs_lookup.remove(tx_id) {
                self.pending_txs_by_index.remove(i);
            }
        }
    }
}
