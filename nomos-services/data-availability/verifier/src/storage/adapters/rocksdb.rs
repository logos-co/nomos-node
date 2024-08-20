// std
use serde::{de::DeserializeOwned, Serialize};
use std::{marker::PhantomData, path::PathBuf};
// crates
use nomos_core::da::blob::Blob;
use nomos_da_storage::{
    fs::write_blob,
    rocksdb::{key_bytes, DA_VERIFIED_KEY_PREFIX},
};
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

pub struct RocksAdapter<A, B, S>
where
    S: StorageSerde + Send + Sync + 'static,
{
    settings: RocksAdapterSettings,
    storage_relay: OutboundRelay<StorageMsg<RocksBackend<S>>>,
    _blob: PhantomData<B>,
    _attestation: PhantomData<A>,
}

#[async_trait::async_trait]
impl<A, B, S> DaStorageAdapter for RocksAdapter<A, B, S>
where
    A: Serialize + DeserializeOwned + Clone + Send + Sync,
    B: Blob + Serialize + Clone + Send + Sync + 'static,
    B::BlobId: AsRef<[u8]> + Send + Sync + 'static,
    B::ColumnIndex: AsRef<[u8]> + Send + Sync + 'static,
    S: StorageSerde + Send + Sync + 'static,
{
    type Backend = RocksBackend<S>;
    type Blob = B;
    type Attestation = A;
    type Settings = RocksAdapterSettings;

    async fn new(
        settings: Self::Settings,
        storage_relay: OutboundRelay<<StorageService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        Self {
            settings,
            storage_relay,
            _blob: PhantomData,
            _attestation: PhantomData,
        }
    }

    async fn add_blob(
        &self,
        blob: &Self::Blob,
        attestation: &Self::Attestation,
    ) -> Result<(), DynError> {
        let blob_bytes = S::serialize(blob);
        let blob_idx = create_blob_idx(blob.id().as_ref(), blob.column_idx().as_ref());

        write_blob(
            self.settings.blob_storage_directory.clone(),
            blob.id().as_ref(),
            blob.column_idx().as_ref(),
            &blob_bytes,
        )
        .await?;

        // Mark blob as attested for lateer use in Indexer and attestation cache.
        let blob_key = key_bytes(DA_VERIFIED_KEY_PREFIX, blob_idx);
        self.storage_relay
            .send(StorageMsg::Store {
                key: blob_key,
                value: S::serialize(attestation),
            })
            .await
            .map_err(|(e, _)| e.into())
    }

    async fn get_blob(
        &self,
        blob_idx: <Self::Blob as Blob>::BlobId,
    ) -> Result<Option<Self::Attestation>, DynError> {
        let key = key_bytes(DA_VERIFIED_KEY_PREFIX, blob_idx);
        let (reply_channel, reply_rx) = tokio::sync::oneshot::channel();
        self.storage_relay
            .send(StorageMsg::Load { key, reply_channel })
            .await
            .expect("Failed to send load request to storage relay");

        // TODO: Use storage backend ser/de functionality.
        //
        // Storage backend already handles ser/de, but lacks the ability to seperate storage
        // domains using prefixed keys. Once implemented Indexer and Verifier can be simplified.
        reply_rx
            .await
            .map(|maybe_bytes| {
                maybe_bytes.map(|bytes| {
                    S::deserialize(bytes).expect("Attestation should be deserialized from bytes")
                })
            })
            .map_err(|_| "".into())
    }
}

fn create_blob_idx(blob_id: &[u8], column_idx: &[u8]) -> [u8; 34] {
    let mut blob_idx = [0u8; 34];
    blob_idx[..blob_id.len()].copy_from_slice(blob_id);
    blob_idx[blob_id.len()..].copy_from_slice(column_idx);

    blob_idx
}

#[derive(Debug, Clone)]
pub struct RocksAdapterSettings {
    pub blob_storage_directory: PathBuf,
}
