// std
use serde::{Deserialize, Serialize};
use std::{marker::PhantomData, path::PathBuf};
// crates
use nomos_core::da::blob::Blob;
use nomos_storage::{
    backends::{rocksdb::RocksBackend, StorageSerde},
    StorageMsg, StorageService,
};
use overwatch_rs::{
    services::{relay::OutboundRelay, ServiceData},
    DynError,
};
// internal
use crate::storage::DaStorageAdapter;

pub struct RocksAdapter<B, S>
where
    S: StorageSerde + Send + Sync + 'static,
{
    _settings: RocksAdapterSettings,
    _storage_relay: OutboundRelay<StorageMsg<RocksBackend<S>>>,
    blob: PhantomData<B>,
}

#[async_trait::async_trait]
impl<B, S> DaStorageAdapter for RocksAdapter<B, S>
where
    S: StorageSerde + Send + Sync + 'static,
    B: Blob + Clone + Send + Sync + 'static,
    B::BlobId: Send,
{
    type Backend = RocksBackend<S>;
    type Blob = B;
    type Settings = RocksAdapterSettings;

    async fn new(
        _settings: Self::Settings,
        _storage_relay: OutboundRelay<<StorageService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        Self {
            _settings,
            _storage_relay,
            blob: PhantomData,
        }
    }

    async fn get_blob(
        &self,
        _blob_id: <Self::Blob as Blob>::BlobId,
    ) -> Result<Option<Self::Blob>, DynError> {
        todo!()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RocksAdapterSettings {
    pub blob_storage_directory: PathBuf,
}
