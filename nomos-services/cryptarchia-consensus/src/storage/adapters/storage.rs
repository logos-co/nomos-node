// STD
use std::hash::Hash;
use std::marker::PhantomData;
// Crates
use nomos_core::block::Block;
use nomos_core::header::HeaderId;
use nomos_mempool::backend::MemPool;
use nomos_storage::backends::StorageBackend;
use nomos_storage::{StorageMsg, StorageService};
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
use serde::de::DeserializeOwned;
// Internal
use crate::storage::StorageAdapter as StorageAdapterTrait;

pub struct StorageAdapter<Storage, ClPool, DaPool>
where
    Storage: StorageBackend + Send + Sync,
{
    storage_relay: OutboundRelay<<StorageService<Storage> as ServiceData>::Message>,
    _cl_pool: PhantomData<ClPool>,
    _da_pool: PhantomData<DaPool>,
}

#[async_trait::async_trait]
impl<Storage, ClPool, DaPool> StorageAdapterTrait for StorageAdapter<Storage, ClPool, DaPool>
where
    Storage: StorageBackend + Send + Sync,
    DaPool: MemPool<BlockId = HeaderId, Item: Clone + Eq + Hash + Send + DeserializeOwned> + Sync,
    ClPool: MemPool<BlockId = HeaderId, Item: Clone + Eq + Hash + Send + DeserializeOwned> + Sync,
{
    type Backend = Storage;
    type ClPool = ClPool;
    type DaPool = DaPool;

    async fn new(
        storage_relay: OutboundRelay<<StorageService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        Self {
            storage_relay,
            _cl_pool: Default::default(),
            _da_pool: Default::default(),
        }
    }

    async fn get_block(
        &self,
        header_id: HeaderId,
    ) -> Option<Block<<Self::ClPool as MemPool>::Item, <Self::DaPool as MemPool>::Item>> {
        let (msg, receiver) = <StorageMsg<Storage>>::new_load_message(header_id);
        self.storage_relay.send(msg).await.unwrap();
        receiver.recv().await.unwrap()
    }

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
        current_block: Block<<Self::ClPool as MemPool>::Item, <Self::DaPool as MemPool>::Item>,
        security_param: &u64,
    ) -> Option<Block<<Self::ClPool as MemPool>::Item, <Self::DaPool as MemPool>::Item>> {
        let mut current_block = current_block;
        // TODO: This implies fetching from DB `security_param` times. We should optimize this.
        for _ in 0..*security_param {
            let parent_block_header = current_block.header().parent();
            let parent_block = self.get_block(parent_block_header).await?;
            current_block = parent_block;
        }
        Some(current_block)
    }
}
