// std
use linked_hash_map::LinkedHashMap;
use nomos_core::da::attestation::Attestation;
use nomos_core::da::certificate::mock::{MockCertVerifier, MockKeyStore};
use nomos_core::da::certificate::verify::{DaCertificateVerifier, KeyStore};
use nomos_core::da::certificate::{Certificate, CertificateVerifier};
use nomos_core::tx::mock::MockTxVerifier;
use nomos_core::tx::Transaction;
use nomos_da::auth::mock::{MockDaAuth, MockDaAuthSettings};
use nomos_da::auth::DaAuth;
use std::hash::Hash;
use std::path::PathBuf;
use std::time::SystemTime;
use std::{collections::BTreeMap, time::UNIX_EPOCH};
// crates
// internal
use crate::backend::{MemPool, MempoolError};
use nomos_core::block::BlockId;

use super::{Status, Verifier};

/// A mock mempool implementation that stores all transactions in memory in the order received.
pub struct MockPool<Item, Key> {
    pending_items: LinkedHashMap<Key, Item>,
    in_block_items: BTreeMap<BlockId, Vec<Item>>,
    in_block_items_by_id: BTreeMap<Key, BlockId>,
    last_item_timestamp: u64,
}

impl<Item, Key> Default for MockPool<Item, Key>
where
    Key: Hash + Eq,
{
    fn default() -> Self {
        Self {
            pending_items: LinkedHashMap::new(),
            in_block_items: BTreeMap::new(),
            in_block_items_by_id: BTreeMap::new(),
            last_item_timestamp: 0,
        }
    }
}

impl<Item, Key> MockPool<Item, Key>
where
    Key: Hash + Eq + Clone,
{
    pub fn new() -> Self {
        Default::default()
    }
}

impl<Item, Key> MemPool for MockPool<Item, Key>
where
    Item: Clone + Send + Sync + 'static + Hash,
    Key: Clone + Ord + Hash,
{
    type Settings = ();
    type Item = Item;
    type Key = Key;

    fn new(_settings: Self::Settings) -> Self {
        Self::default()
    }

    fn add_item<V: Verifier<Self::Item>>(
        &mut self,
        key: Self::Key,
        item: Self::Item,
        verifier: &V,
    ) -> Result<(), MempoolError> {
        if self.pending_items.contains_key(&key) || self.in_block_items_by_id.contains_key(&key) {
            return Err(MempoolError::ExistingItem);
        }
        if !verifier.verify(&item) {
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
    type Settings = ();

    fn new(_: Self::Settings) -> Self {
        Default::default()
    }

    fn verify(&self, item: &T) -> bool {
        self.verify_tx(item)
    }
}

impl<C: Certificate> Verifier<C> for MockCertVerifier {
    type Settings = ();

    fn new(_: Self::Settings) -> Self {
        Default::default()
    }

    fn verify(&self, item: &C) -> bool {
        self.verify_cert(item)
    }
}

#[derive(Clone, Debug)]
pub struct MockDaVerifierSettings {
    pub node_keys: Vec<([u8; 32], PathBuf)>,
}

impl<K, C> Verifier<C> for DaCertificateVerifier<K, MockKeyStore<MockDaAuth>, C>
where
    C: Certificate + Clone,
    <<C as Certificate>::Attestation as Attestation>::Voter: Into<K> + Clone,
    <<C as Certificate>::Attestation as Attestation>::Hash: AsRef<[u8]>,
    MockKeyStore<MockDaAuth>: KeyStore<K> + 'static,
    <MockKeyStore<MockDaAuth> as KeyStore<K>>::Verifier: 'static,
{
    type Settings = MockDaVerifierSettings;

    fn new(settings: Self::Settings) -> Self {
        // TODO: Mempool needs to verify that certificates are composed of attestations signed by
        // valid da nodes. To verify that, node needs to maintain a list of DA Node public keys. At
        // the moment we are using mock key store, which implements KeyStore trait. The main
        // functionality of this trait is to get public key which could be used for signature
        // verification. In the future public key retrieval might be implemented as a seperate
        // Overwatch service.
        let mut store = MockKeyStore::default();
        for (node, key_path) in settings.node_keys {
            let key = <MockDaAuth as DaAuth>::new(MockDaAuthSettings {
                pkcs8_file_path: key_path,
            });
            store.add_key(&node, key);
        }
        DaCertificateVerifier::new(store)
    }

    fn verify(&self, item: &C) -> bool {
        CertificateVerifier::verify(self, item)
    }
}
