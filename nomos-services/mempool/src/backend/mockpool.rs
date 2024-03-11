// std
use linked_hash_map::LinkedHashMap;
use std::hash::Hash;
use std::time::SystemTime;
use std::{collections::BTreeMap, time::UNIX_EPOCH};
// crates
// internal
use crate::backend::{MemPool, MempoolError};

use super::Status;

/// A mock mempool implementation that stores all transactions in memory in the order received.
pub struct MockPool<BlockId, Item, Key> {
    pending_items: LinkedHashMap<Key, Item>,
    in_block_items: BTreeMap<BlockId, Vec<Item>>,
    in_block_items_by_id: BTreeMap<Key, BlockId>,
    last_item_timestamp: u64,
}

impl<BlockId, Item, Key> Default for MockPool<BlockId, Item, Key>
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

impl<BlockId, Item, Key> MockPool<BlockId, Item, Key>
where
    Key: Hash + Eq + Clone,
{
    pub fn new() -> Self {
        Default::default()
    }
}

impl<BlockId, Item, Key> MemPool for MockPool<BlockId, Item, Key>
where
    Item: Clone + Send + Sync + 'static + Hash,
    Key: Clone + Ord + Hash,
    BlockId: Copy + Ord,
{
    type Settings = ();
    type Item = Item;
    type Key = Key;
    type BlockId = BlockId;

    fn new(_settings: Self::Settings) -> Self {
        Self::new()
    }

    fn add_item(&mut self, key: Self::Key, item: Self::Item) -> Result<(), MempoolError> {
        if self.pending_items.contains_key(&key) || self.in_block_items_by_id.contains_key(&key) {
            return Err(MempoolError::ExistingItem);
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

    fn status(&self, items: &[Self::Key]) -> Vec<Status<BlockId>> {
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
