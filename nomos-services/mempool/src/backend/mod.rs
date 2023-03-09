#[cfg(feature = "mock")]
pub mod mockpool;

use nomos_core::block::{BlockHeader, BlockId};

#[derive(thiserror::Error, Debug)]
pub enum MempoolError {
    #[error("Tx already in mempool")]
    ExistingTx,
    #[error(transparent)]
    DynamicPoolError(#[from] overwatch_rs::DynError),
}

pub trait MemPool {
    type Settings: Clone;
    type Tx;
    type Id;

    /// Construct a new empty pool
    fn new(settings: Self::Settings) -> Self;

    /// Add a new transaction to the mempool, for example because we received it from the network
    fn add_tx(&mut self, tx: Self::Tx) -> Result<(), MempoolError>;

    /// Return a view over the transactions contained in the mempool.
    /// Implementations should provide *at least* all the transactions which have not been marked as
    /// in a block.
    /// The hint on the ancestor *can* be used by the implementation to display additional
    /// transactions that were not included up to that point if available.
    fn view(&self, ancestor_hint: BlockId) -> Box<dyn Iterator<Item = Self::Tx> + Send>;

    /// Record that a set of transactions were included in a block
    fn mark_in_block(&mut self, txs: &[Self::Id], block: BlockHeader);

    /// Returns all of the transactions for the block
    fn block_transactions(
        &self,
        block: BlockId,
    ) -> Option<Box<dyn Iterator<Item = Self::Tx> + Send>>;

    /// Signal that a set of transactions can't be possibly requested anymore and can be
    /// discarded.
    fn prune(&mut self, txs: &[Self::Id]);

    fn pending_tx_count(&self) -> usize;
    fn last_tx_timestamp(&self) -> u64;
}
