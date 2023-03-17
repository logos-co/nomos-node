// std
use linked_hash_map::LinkedHashMap;
use std::hash::Hash;
use std::time::SystemTime;
use std::{collections::BTreeMap, time::UNIX_EPOCH};
// crates
// internal
use crate::backend::{MemPool, MempoolError};
use nomos_core::block::{BlockHeader, BlockId};
use nomos_core::tx::Transaction;

/// A mock mempool implementation that stores all transactions in memory in the order received.
pub struct MockPool<Tx: Transaction>
where
    Tx::Hash: Hash,
{
    pending_txs: LinkedHashMap<Tx::Hash, Tx>,
    in_block_txs: BTreeMap<BlockId, Vec<Tx>>,
    in_block_txs_by_id: BTreeMap<Tx::Hash, BlockId>,
    last_tx_timestamp: u64,
}

impl<Tx: Transaction> Default for MockPool<Tx> {
    fn default() -> Self {
        Self {
            pending_txs: LinkedHashMap::new(),
            in_block_txs: BTreeMap::new(),
            in_block_txs_by_id: BTreeMap::new(),
            last_tx_timestamp: 0,
        }
    }
}

impl<Tx: Transaction> MockPool<Tx>
where
    Tx::Hash: Ord,
{
    pub fn new() -> Self {
        Default::default()
    }
}

impl<Tx> MemPool for MockPool<Tx>
where
    Tx: Transaction + Clone + Send + Sync + 'static + Hash,
    Tx::Hash: Ord,
{
    type Settings = ();
    type Tx = Tx;

    fn new(_settings: Self::Settings) -> Self {
        Self::new()
    }

    fn add_tx(&mut self, tx: Self::Tx) -> Result<(), MempoolError> {
        let id = <Self::Tx as Transaction>::hash(&tx);
        if self.pending_txs.contains_key(&id) || self.in_block_txs_by_id.contains_key(&id) {
            return Err(MempoolError::ExistingTx);
        }
        self.pending_txs.insert(id, tx);
        self.last_tx_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Ok(())
    }

    fn view(&self, _ancestor_hint: BlockId) -> Box<dyn Iterator<Item = Self::Tx> + Send> {
        // we need to have an owned version of the iterator to bypass adding a lifetime bound to the return iterator type
        #[allow(clippy::needless_collect)]
        let pending_txs: Vec<Tx> = self.pending_txs.values().cloned().collect();
        Box::new(pending_txs.into_iter())
    }

    fn mark_in_block(&mut self, txs: &[<Self::Tx as Transaction>::Hash], block: BlockHeader) {
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

    #[cfg(feature = "mock")]
    fn block_transactions(
        &self,
        block: BlockId,
    ) -> Option<Box<dyn Iterator<Item = Self::Tx> + Send>> {
        self.in_block_txs.get(&block).map(|txs| {
            Box::new(txs.clone().into_iter()) as Box<dyn Iterator<Item = Self::Tx> + Send>
        })
    }

    fn prune(&mut self, txs: &[<Self::Tx as Transaction>::Hash]) {
        for tx_id in txs {
            self.pending_txs.remove(tx_id);
        }
    }

    fn pending_tx_count(&self) -> usize {
        self.pending_txs.len()
    }

    fn last_tx_timestamp(&self) -> u64 {
        self.last_tx_timestamp
    }
}
