#[cfg(feature = "mock")]
pub mod mockpool;

use nomos_core::block::BlockId;
use serde::{Deserialize, Serialize};

#[derive(thiserror::Error, Debug)]
pub enum MempoolError {
    #[error("Item already in mempool")]
    ExistingItem,
    #[error(transparent)]
    DynamicPoolError(#[from] overwatch_rs::DynError),
}

pub trait MemPool {
    type Settings: Clone;
    type Item;
    type Key;

    /// Construct a new empty pool
    fn new(settings: Self::Settings) -> Self;

    /// Add a new item to the mempool, for example because we received it from the network
    fn add_item(&mut self, key: Self::Key, item: Self::Item) -> Result<(), MempoolError>;

    /// Return a view over items contained in the mempool.
    /// Implementations should provide *at least* all the items which have not been marked as
    /// in a block.
    /// The hint on the ancestor *can* be used by the implementation to display additional
    /// items that were not included up to that point if available.
    fn view(&self, ancestor_hint: BlockId) -> Box<dyn Iterator<Item = Self::Item> + Send>;

    /// Record that a set of items were included in a block
    fn mark_in_block(&mut self, items: &[Self::Key], block: BlockId);

    /// Returns all of the transactions for the block
    #[cfg(test)]
    fn block_items(&self, block: BlockId) -> Option<Box<dyn Iterator<Item = Self::Item> + Send>>;

    /// Signal that a set of transactions can't be possibly requested anymore and can be
    /// discarded.
    fn prune(&mut self, items: &[Self::Key]);

    fn pending_item_count(&self) -> usize;
    fn last_item_timestamp(&self) -> u64;

    // Return the status of a set of items.
    // This is a best effort attempt, and implementations are free to return `Unknown` for all of them.
    fn status(&self, items: &[Self::Key]) -> Vec<Status>;
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum Status {
    /// Unknown status
    Unknown,
    /// Pending status
    Pending,
    /// Rejected status
    Rejected,
    /// Accepted status
    ///
    /// The block id of the block that contains the item
    #[cfg_attr(
        feature = "openapi",
        schema(
            example = "e.g. 0x"
        )
    )]
    InBlock { block: BlockId },
}
