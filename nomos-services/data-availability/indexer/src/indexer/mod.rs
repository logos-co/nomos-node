use nomos_core::da::certificate::{metadata::Metadata, vid::VID};

use crate::storage::DaStorageAdapter;

pub struct Range<Index> {
    pub from: Index,
    pub to: Index,
}

#[async_trait::async_trait]
pub trait DaIndexer {
    type Settings: Clone;
    type Blob;
    type VID: VID;
    type Storage: DaStorageAdapter;

    fn new(settings: Self::Settings) -> Self;
    async fn add_index(&self, vid: Self::VID, storage: Self::Storage) -> bool;
    async fn get_range(
        &self,
        range: Range<<Self::VID as Metadata>::Index>,
    ) -> Vec<Option<Self::Blob>>;
}
