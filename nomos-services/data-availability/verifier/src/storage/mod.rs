pub mod adapters;

use nomos_core::da::blob::Blob;
use nomos_storage::{backends::StorageBackend, StorageService};
use overwatch_rs::{
    services::{relay::OutboundRelay, ServiceData},
    DynError,
};

#[async_trait::async_trait]
pub trait DaStorageAdapter {
    type Backend: StorageBackend + Send + Sync + 'static;
    type Settings: Clone;
    type Blob: Blob + Clone;
    type Attestation: Clone;

    async fn new(
        settings: Self::Settings,
        storage_relay: OutboundRelay<<StorageService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;

    async fn add_blob(
        &self,
        blob: &Self::Blob,
        attestation: &Self::Attestation,
    ) -> Result<(), DynError>;

    async fn get_blob(
        &self,
        blob_id: <Self::Blob as Blob>::BlobId,
        column_idx: <Self::Blob as Blob>::ColumnIndex,
    ) -> Result<Option<Self::Attestation>, DynError>;
}
