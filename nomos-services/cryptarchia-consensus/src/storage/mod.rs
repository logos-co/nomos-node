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
}
