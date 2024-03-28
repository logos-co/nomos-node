use nomos_storage::{backends::StorageBackend, StorageService};
use overwatch_rs::services::{relay::OutboundRelay, ServiceData};

#[async_trait::async_trait]
pub trait DaStorageAdapter {
    type Backend: StorageBackend + Send + Sync + 'static;

    type Blob;
    type VID;

    fn new(
        storage_relay: OutboundRelay<<StorageService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;

    async fn add_index(&self);
    async fn get_range(&self);
}
