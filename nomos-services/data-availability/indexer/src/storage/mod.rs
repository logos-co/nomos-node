use std::ops::Range;

use nomos_core::da::certificate::{metadata::Metadata, vid::VID};
use nomos_storage::{backends::StorageBackend, StorageService};
use overwatch_rs::services::{relay::OutboundRelay, ServiceData};

#[async_trait::async_trait]
pub trait DaStorageAdapter {
    type Backend: StorageBackend + Send + Sync + 'static;

    type Blob;
    type VID: VID;

    async fn new(
        storage_relay: OutboundRelay<<StorageService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;

    async fn add_index(&self, vid: Self::VID) -> bool;
    async fn get_range(
        &self,
        range: Range<<Self::VID as Metadata>::Index>,
    ) -> Vec<Option<Self::Blob>>;
}
