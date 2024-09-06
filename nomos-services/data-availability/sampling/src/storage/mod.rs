pub mod adapters;

use kzgrs_backend::common::ColumnIndex;
use nomos_core::da::blob::Blob;
use nomos_storage::{backends::StorageBackend, StorageService};
use overwatch_rs::{
    services::{relay::OutboundRelay, ServiceData},
    DynError,
};

#[async_trait::async_trait]
pub trait DaStorageAdapter {
    type Backend: StorageBackend + Send + Sync + 'static;
    type Settings: Clone + Send + Sync + 'static;
    type Blob: Blob + Clone;

    async fn new(
        settings: Self::Settings,
        storage_relay: OutboundRelay<<StorageService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;

    async fn get_blob(
        &self,
        blob_id: <Self::Blob as Blob>::BlobId,
        column_idx: ColumnIndex,
    ) -> Result<Option<Self::Blob>, DynError>;
}
