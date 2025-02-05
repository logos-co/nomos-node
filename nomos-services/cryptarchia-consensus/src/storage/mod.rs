pub mod adapters;

// STD
use std::hash::Hash;
// Crates
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
// Internal
use nomos_core::block::Block;
use nomos_core::header::HeaderId;
use nomos_mempool::backend::MemPool;
use nomos_storage::backends::StorageBackend;
use nomos_storage::StorageService;

#[async_trait::async_trait]
pub trait StorageAdapter {
    type Backend: StorageBackend + Send + Sync;
    type ClPool: MemPool<BlockId = HeaderId, Item: Clone + Eq + Hash>;
    type DaPool: MemPool<BlockId = HeaderId, Item: Clone + Eq + Hash>;

    async fn new(
        network_relay: OutboundRelay<<StorageService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;

    async fn get_block(
        &self,
        header_id: HeaderId,
    ) -> Option<Block<<Self::ClPool as MemPool>::Item, <Self::DaPool as MemPool>::Item>>;

    async fn get_block_for_security_param(
        &self,
        current_block: Block<<Self::ClPool as MemPool>::Item, <Self::DaPool as MemPool>::Item>,
        security_param: &u64,
    ) -> Option<Block<<Self::ClPool as MemPool>::Item, <Self::DaPool as MemPool>::Item>>;
}
