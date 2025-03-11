pub mod adapters;

use nomos_core::da::blob::Blob;
use nomos_storage::{backends::StorageBackend, StorageService};
use overwatch::{
    services::{relay::OutboundRelay, ServiceData},
    DynError,
};

#[async_trait::async_trait]
pub trait DaStorageAdapter {
    type Backend: StorageBackend + Send + Sync + 'static;
    type Settings: Clone;
    type Blob: Blob + Clone;

    async fn new(
        storage_relay: OutboundRelay<<StorageService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;

    async fn add_blob(
        &self,
        blob_id: <Self::Blob as Blob>::BlobId,
        column_idx: <Self::Blob as Blob>::ColumnIndex,
        commitments: <Self::Blob as Blob>::SharedCommitments,
        light_blob: <Self::Blob as Blob>::LightBlob,
    ) -> Result<(), DynError>;

    async fn get_blob(
        &self,
        blob_id: <Self::Blob as Blob>::BlobId,
        column_idx: <Self::Blob as Blob>::ColumnIndex,
    ) -> Result<Option<<Self::Blob as Blob>::LightBlob>, DynError>;
}
