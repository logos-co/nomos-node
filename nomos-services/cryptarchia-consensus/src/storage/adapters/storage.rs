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
    pub storage_relay: OutboundRelay<<StorageService<Storage> as ServiceData>::Message>,
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

    async fn save_security_block(&self, block: Block<ClPool::Item, DaPool::Item>) {
        let security_block_msg =
            <StorageMsg<_>>::new_store_message("security_block_header_id", block.header().id());

        if let Err((e, _msg)) = self.storage_relay.send(security_block_msg).await {
            tracing::error!("Could not send security block id to storage: {e}");
        }
    }
}
