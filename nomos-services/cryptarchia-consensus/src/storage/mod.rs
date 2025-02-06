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

    /// Sends a store message to the storage service to fetch the security block header
    ///
    /// # Returns
    ///
    /// The header id of the security block. If no security block is found, returns None.
    async fn get_security_block_header(&self) -> Option<HeaderId>;

    /// Sends a store message to the storage service to save a block id as the security block
    /// A security block is the one that is `security_param` blocks behind the current block
    /// This is used to rebuild the state in case of a crash
    ///
    /// This operation might fail
    ///
    /// # Arguments
    ///
    /// * `header_id` - The header id of the block to mark as the security block
    async fn save_header_as_security_block(&self, header_id: &HeaderId);
}
