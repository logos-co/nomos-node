// std
use linked_hash_map::LinkedHashMap;
use nomos_core::da::attestation::Attestation;
use nomos_core::da::certificate::mock::MockCertVerifier;
use nomos_core::da::certificate::verify::{DaCertificateVerifier, KeyStore};
use nomos_core::da::certificate::{Certificate, CertificateVerifier};
use nomos_core::tx::mock::MockTxVerifier;
use nomos_core::tx::Transaction;
use std::hash::Hash;
use std::time::SystemTime;
use std::{collections::BTreeMap, time::UNIX_EPOCH};
// crates
// internal
use crate::backend::{MemPool, MempoolError};
use nomos_core::block::BlockId;

use super::{Status, Verifier};

/// A mock mempool implementation that stores all transactions in memory in the order received.
pub struct MockPool<Item, Key, Verifier> {
    pending_items: LinkedHashMap<Key, Item>,
    in_block_items: BTreeMap<BlockId, Vec<Item>>,
    in_block_items_by_id: BTreeMap<Key, BlockId>,
    last_item_timestamp: u64,
    verifier: Option<Verifier>,
}

impl<Item, Key, Verifier> Default for MockPool<Item, Key, Verifier>
where
    Key: Hash + Eq,
{
    fn default() -> Self {
        Self {
            pending_items: LinkedHashMap::new(),
            in_block_items: BTreeMap::new(),
            in_block_items_by_id: BTreeMap::new(),
            last_item_timestamp: 0,
            verifier: None,
        }
    }
}

impl<Item, Key, Verifier> MockPool<Item, Key, Verifier>
where
    Key: Hash + Eq + Clone,
{
    pub fn new() -> Self {
        Default::default()
    }
}

impl<Item, Key, V> MemPool for MockPool<Item, Key, V>
where
    Item: Clone + Send + Sync + 'static + Hash,
    Key: Clone + Ord + Hash,
    V: Verifier<Item>,
{
    type Settings = ();
    type Item = Item;
    type Key = Key;
    type Verifier = V;

    fn new(_settings: Self::Settings) -> Self {
        Self::new()
    }

    fn add_item(&mut self, key: Self::Key, item: Self::Item) -> Result<(), MempoolError> {
        if self.pending_items.contains_key(&key) || self.in_block_items_by_id.contains_key(&key) {
            return Err(MempoolError::ExistingItem);
        }
        if !self
            .verifier
            .as_ref()
            .map(|verifier| verifier.verify(&item))
            .unwrap_or(true)
        {
            return Err(MempoolError::VerificationError);
        }
        self.pending_items.insert(key, item);
        self.last_item_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Ok(())
    }

    fn view(&self, _ancestor_hint: BlockId) -> Box<dyn Iterator<Item = Self::Item> + Send> {
        // we need to have an owned version of the iterator to bypass adding a lifetime bound to the return iterator type
        #[allow(clippy::needless_collect)]
        let pending_items: Vec<Item> = self.pending_items.values().cloned().collect();
        Box::new(pending_items.into_iter())
    }

    fn mark_in_block(&mut self, keys: &[Self::Key], block: BlockId) {
        let mut items_in_block = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some(item) = self.pending_items.remove(key) {
                items_in_block.push(item);
            }
        }
        let block_entry = self.in_block_items.entry(block).or_default();
        self.in_block_items_by_id
            .extend(keys.iter().cloned().map(|key| (key, block)));
        block_entry.append(&mut items_in_block);
    }

    #[cfg(test)]
    fn block_items(&self, block: BlockId) -> Option<Box<dyn Iterator<Item = Self::Item> + Send>> {
        self.in_block_items.get(&block).map(|items| {
            Box::new(items.clone().into_iter()) as Box<dyn Iterator<Item = Self::Item> + Send>
        })
    }

    fn prune(&mut self, keys: &[Self::Key]) {
        for key in keys {
            self.pending_items.remove(key);
        }
    }

    fn pending_item_count(&self) -> usize {
        self.pending_items.len()
    }

    fn last_item_timestamp(&self) -> u64 {
        self.last_item_timestamp
    }

    fn status(&self, items: &[Self::Key]) -> Vec<Status> {
        items
            .iter()
            .map(|key| {
                if self.pending_items.contains_key(key) {
                    Status::Pending
                } else if let Some(block) = self.in_block_items_by_id.get(key) {
                    Status::InBlock { block: *block }
                } else {
                    Status::Unknown
                }
            })
            .collect()
    }
}

impl<T: Transaction> Verifier<T> for MockTxVerifier {
    fn verify(&self, item: &T) -> bool {
        self.verify_tx(item)
    }
}

impl<C: Certificate> Verifier<C> for MockCertVerifier {
    fn verify(&self, item: &C) -> bool {
        self.verify_cert(item)
    }
}

impl<C, K, KS> Verifier<C> for DaCertificateVerifier<K, KS, C>
where
    C: Certificate + Clone,
    <<C as Certificate>::Attestation as Attestation>::Voter: Into<K> + Clone,
    <<C as Certificate>::Attestation as Attestation>::Hash: AsRef<[u8]>,
    KS: KeyStore<K> + Clone + 'static,
    KS::Verifier: 'static,
{
    fn verify(&self, item: &C) -> bool {
        CertificateVerifier::verify(self, item)
    }
}
