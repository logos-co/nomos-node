pub mod adapters;

use std::ops::Range;

use futures::Stream;
use nomos_core::da::certificate::{metadata::Metadata, vid::VidCertificate};
use nomos_storage::{backends::StorageBackend, StorageService};
use overwatch_rs::{
    services::{relay::OutboundRelay, ServiceData},
    DynError,
};

#[async_trait::async_trait]
pub trait DaStorageAdapter {
    type Backend: StorageBackend + Send + Sync + 'static;
    type Settings: Clone;

    type Blob;
    type VID: VidCertificate;

    async fn new(
        config: Self::Settings,
        storage_relay: OutboundRelay<<StorageService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;

    async fn add_index(&self, vid: &Self::VID) -> Result<(), DynError>;
    async fn get_range_stream(
        &self,
        app_id: <Self::VID as Metadata>::AppId,
        range: Range<<Self::VID as Metadata>::Index>,
    ) -> Box<dyn Stream<Item = (<Self::VID as Metadata>::Index, Option<Self::Blob>)> + Unpin + Send>;
}
