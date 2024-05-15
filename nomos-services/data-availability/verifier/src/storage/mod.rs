pub mod adapters;

use nomos_storage::{backends::StorageBackend, StorageService};
use overwatch_rs::{
    services::{relay::OutboundRelay, ServiceData},
    DynError,
};

#[async_trait::async_trait]
pub trait DaStorageAdapter {
    type Backend: StorageBackend + Send + Sync + 'static;
    type Settings: Clone;
    type Blob: Clone;
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
    async fn get_attestation(
        &self,
        blob: &Self::Blob,
    ) -> Result<Option<Self::Attestation>, DynError>;
}
