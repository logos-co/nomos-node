// std
use linked_hash_map::LinkedHashMap;
use std::collections::BTreeMap;
// crates
// internal
use crate::backend::MemPool;
use nomos_core::block::{BlockHeader, BlockId};

/// A mock mempool implementation that stores all transactions in memory in the order received.
pub struct MockPool<Id, Tx> {
    pending_txs: LinkedHashMap<Id, Tx>,
    in_block_txs: LinkedHashMap<BlockId, Vec<Tx>>,
}

impl<Id, Tx> Default for MockPool<Id, Tx> {
    fn default() -> Self {
        Self {
            pending_txs: LinkedHashMap::new(),
            in_block_txs: LinkedHashMap::new(),
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
        if self.pending_txs.contains_key(&id) || self.in_block_txs.contains_key(&id) {
            return Ok(());
        }
        self.pending_txs.insert(id, tx);
        Ok(())
    }

    fn view(&self, _ancestor_hint: BlockId) -> Box<dyn Iterator<Item = Self::Tx> + Send> {
        // we need to have an owned version of the iterator to bypass adding a lifetime bound to the return iterator type
        #[allow(clippy::unnecessary_to_owned)]
        Box::new(self.pending_txs.to_vec().into_iter())
    }

    fn mark_in_block(&mut self, txs: &[Self::Id], block: BlockHeader) {
        let mut txs_in_block = Vec::with_capacity(txs.len());
        for tx_id in txs.iter() {
            if let Some(tx) = self.pending_txs.remove(tx_id) {
                txs_in_block.push(tx);
            }
        }
        let block_entry = self.in_block_txs.entry(block.id).or_default();
        block_entry.append(&mut txs_in_block);
    }

    fn prune(&mut self, txs: &[Self::Id]) {
        for tx_id in txs {
            self.pending_txs.remove(tx_id)
        }
    }
}
