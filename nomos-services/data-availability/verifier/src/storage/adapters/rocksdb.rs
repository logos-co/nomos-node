// std
use std::{marker::PhantomData, path::PathBuf};
// crates
use futures::try_join;
use nomos_core::da::blob::Blob;
use nomos_da_storage::rocksdb::{create_blob_idx, key_bytes, DA_VERIFIED_KEY_PREFIX};
use nomos_da_storage::rocksdb::{DA_BLOB_PATH, DA_SHARED_COMMITMENTS_PATH};
use nomos_storage::{
    backends::{rocksdb::RocksBackend, StorageSerde},
    StorageMsg, StorageService,
};
use overwatch_rs::{
    services::{relay::OutboundRelay, ServiceData},
    DynError,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
// internal
use crate::storage::DaStorageAdapter;

pub struct RocksAdapter<B, S>
where
    S: StorageSerde + Send + Sync + 'static,
{
    storage_relay: OutboundRelay<StorageMsg<RocksBackend<S>>>,
    _blob: PhantomData<B>,
}

#[async_trait::async_trait]
impl<B, S> DaStorageAdapter for RocksAdapter<B, S>
where
    B: Blob + Clone + Send + Sync + 'static,
    B::BlobId: AsRef<[u8]> + Send + Sync + 'static,
    B::ColumnIndex: AsRef<[u8]> + Send + Sync + 'static,
    B::LightBlob: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    B::SharedCommitments: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    S: StorageSerde + Send + Sync + 'static,
{
    type Backend = RocksBackend<S>;
    type Blob = B;
    type Settings = RocksAdapterSettings;

    async fn new(
        storage_relay: OutboundRelay<<StorageService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        Self {
            storage_relay,
            _blob: PhantomData,
        }
    }

    async fn add_blob(&self, blob: Self::Blob) -> Result<(), DynError> {
        let blob_id = blob.id();
        let column_idx = blob.column_idx();
        let (light_blob, shared_commitments) = blob.into_blob_and_shared_commitments();

        try_join!(
            {
                // Store the blob in the storage backend.
                let blob_prefix = format!("{}{}", DA_VERIFIED_KEY_PREFIX, DA_BLOB_PATH);
                let blob_idx = create_blob_idx(blob_id.as_ref(), column_idx.as_ref());
                let blob_key = key_bytes(&blob_prefix, blob_idx);
                self.storage_relay.send(StorageMsg::Store {
                    key: blob_key,
                    value: S::serialize(light_blob),
                })
            },
            {
                let shared_commitments_prefix =
                    format!("{}{}", DA_VERIFIED_KEY_PREFIX, DA_SHARED_COMMITMENTS_PATH);
                let shared_commitments_key = key_bytes(&shared_commitments_prefix, &blob_id);
                self.storage_relay.send(StorageMsg::Store {
                    key: shared_commitments_key,
                    value: S::serialize(shared_commitments),
                })
            }
        )
        .map(|_| ())
        .map_err(|e| format!("Failed to store blob in storage adapter: {e:?}").into())
    }

    async fn get_blob(
        &self,
        blob_id: <Self::Blob as Blob>::BlobId,
        column_idx: <Self::Blob as Blob>::ColumnIndex,
    ) -> Result<Option<<Self::Blob as Blob>::LightBlob>, DynError> {
        let blob_prefix = format!("{}{}", DA_VERIFIED_KEY_PREFIX, DA_BLOB_PATH);
        let blob_idx = create_blob_idx(blob_id.as_ref(), column_idx.as_ref());
        let blob_key = key_bytes(&blob_prefix, blob_idx);
        let (reply_channel, reply_rx) = tokio::sync::oneshot::channel();
        self.storage_relay
            .send(StorageMsg::Load {
                key: blob_key,
                reply_channel,
            })
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
                    S::deserialize(bytes).expect("Blob should be deserialized from bytes")
                })
            })
            .map_err(|_| "".into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RocksAdapterSettings {
    pub blob_storage_directory: PathBuf,
}
