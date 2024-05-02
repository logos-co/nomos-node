use std::ops::Range;

use nomos_core::da::certificate::{metadata::Metadata, vid::VID};
use overwatch_rs::DynError;

use crate::storage::DaStorageAdapter;

#[async_trait::async_trait]
pub trait DaIndexer {
    type Settings: Clone;
    type Blob;
    type VID: VID;
    type Storage: DaStorageAdapter;

    fn new(settings: Self::Settings) -> Self;
    async fn add_index(&self, vid: &Self::VID, storage: &Self::Storage) -> Result<(), DynError>;
    async fn get_range(
        &self,
        app_id: <Self::VID as Metadata>::AppId,
        range: Range<<Self::VID as Metadata>::Index>,
        storage: &Self::Storage,
    ) -> Result<Vec<Option<Self::Blob>>, DynError>;
}
