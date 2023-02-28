// std
use linked_hash_map::LinkedHashMap;
use std::hash::Hash;
use std::time::SystemTime;
use std::{collections::BTreeMap, time::UNIX_EPOCH};
// crates
// internal
use crate::backend::{MemPool, MempoolError};
use nomos_core::block::{BlockHeader, BlockId};

/// A mock mempool implementation that stores all transactions in memory in the order received.
pub struct MockPool<Id, Tx> {
    pending_txs: LinkedHashMap<Id, Tx>,
    in_block_txs: BTreeMap<BlockId, Vec<Tx>>,
    in_block_txs_by_id: BTreeMap<Id, BlockId>,
    last_tx_timestamp: u128,
}

impl<Id, Tx> Default for MockPool<Id, Tx>
where
    Id: Eq + Hash,
{
    fn default() -> Self {
        Self {
            pending_txs: LinkedHashMap::new(),
            in_block_txs: BTreeMap::new(),
            in_block_txs_by_id: BTreeMap::new(),
            last_tx_timestamp: 0,
        }
    }
}

impl<Id, Tx> MockPool<Id, Tx>
where
    Id: Eq + Hash,
{
    pub fn new() -> Self {
        Default::default()
    }
}

impl<Id, Tx> MemPool for MockPool<Id, Tx>
where
    Id: for<'t> From<&'t Tx> + PartialOrd + Ord + Eq + Hash + Clone,
    Tx: Clone + Send + Sync + 'static + Hash,
{
    type Settings = ();
    type Tx = Tx;
    type Id = Id;

    fn new(_settings: Self::Settings) -> Self {
        Self::new()
    }

    fn add_tx(&mut self, tx: Self::Tx) -> Result<(), MempoolError> {
        let id = Id::from(&tx);
        if self.pending_txs.contains_key(&id) || self.in_block_txs_by_id.contains_key(&id) {
            return Err(MempoolError::ExistingTx);
        }
        self.pending_txs.insert(id, tx);
        self.last_tx_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();

        Ok(())
    }

    fn view(&self, _ancestor_hint: BlockId) -> Box<dyn Iterator<Item = Self::Tx> + Send> {
        // we need to have an owned version of the iterator to bypass adding a lifetime bound to the return iterator type
        #[allow(clippy::needless_collect)]
        let pending_txs: Vec<Tx> = self.pending_txs.values().cloned().collect();
        Box::new(pending_txs.into_iter())
    }

    fn mark_in_block(&mut self, txs: &[Self::Id], block: BlockHeader) {
        let mut txs_in_block = Vec::with_capacity(txs.len());
        for tx_id in txs.iter() {
            if let Some(tx) = self.pending_txs.remove(tx_id) {
                txs_in_block.push(tx);
            }
        }
        let block_entry = self.in_block_txs.entry(block.id()).or_default();
        self.in_block_txs_by_id
            .extend(txs.iter().cloned().map(|tx| (tx, block.id())));
        block_entry.append(&mut txs_in_block);
    }

    fn prune(&mut self, txs: &[Self::Id]) {
        for tx_id in txs {
            self.pending_txs.remove(tx_id);
        }
    }

    fn pending_tx_count(&self) -> usize {
        self.pending_txs.len()
    }

    fn last_tx_timestamp(&self) -> u128 {
        self.last_tx_timestamp
    }
}
