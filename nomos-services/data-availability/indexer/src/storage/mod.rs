pub mod adapters;

use std::ops::Range;

use futures::Stream;
use nomos_core::da::blob::{info::DispersedBlobInfo, metadata::Metadata};
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
    type Info: DispersedBlobInfo;

    async fn new(
        config: Self::Settings,
        storage_relay: OutboundRelay<<StorageService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;

    async fn add_index(&self, vid: &Self::Info) -> Result<(), DynError>;
    async fn get_range_stream(
        &self,
        app_id: <Self::Info as Metadata>::AppId,
        range: Range<<Self::Info as Metadata>::Index>,
    ) -> Box<dyn Stream<Item = (<Self::Info as Metadata>::Index, Vec<Self::Blob>)> + Unpin + Send>;
}
