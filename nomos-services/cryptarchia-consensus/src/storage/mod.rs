pub mod adapters;

// Crates
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
// Internal
use nomos_core::header::HeaderId;
use nomos_storage::backends::StorageBackend;
use nomos_storage::StorageService;

#[async_trait::async_trait]
pub trait StorageAdapter {
    type Backend: StorageBackend + Send + Sync;
    type Block;

    async fn new(
        network_relay: OutboundRelay<<StorageService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;

    /// Sends a store message to the storage service to retrieve a block by its header id
    ///
    /// # Returns
    ///
    /// The block with the given header id. If no block is found, returns None.
    async fn get_block(&self, key: &HeaderId) -> Option<Self::Block>;

    /// Get the block for a given security parameter (k)
    /// This function will return the block that is `security_param` blocks behind the given block.
    /// If the block is not found, it will return None.
    ///
    /// # Arguments
    ///
    /// * `current_block` - The block to start from. Must be the latest block.
    /// * `security_param` - The number of blocks to go back.
    ///     This is the number of blocks that are considered stable.
    ///
    /// # Returns
    ///
    /// * `Option<Block>` - The block that is `security_param` blocks behind the given block.
    ///     If there are not enough blocks to go back or the block is not found, it will return None.
    async fn get_block_for_security_param(
        &self,
        current_block: Self::Block,
        security_param: &u64,
    ) -> Option<Self::Block>;
}
