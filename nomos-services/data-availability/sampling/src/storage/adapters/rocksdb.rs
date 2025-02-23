// internal
use crate::storage::DaStorageAdapter;
use kzgrs_backend::common::ColumnIndex;
// crates
use nomos_core::da::blob::Blob;
use nomos_da_storage::rocksdb::{key_bytes, DA_VERIFIED_KEY_PREFIX};
use nomos_da_storage::rocksdb::{DA_BLOB_PATH, DA_SHARED_COMMITMENTS_PATH};
use nomos_storage::{
    backends::{rocksdb::RocksBackend, StorageSerde},
    StorageMsg, StorageService,
};
use overwatch_rs::{
    services::{relay::OutboundRelay, ServiceData},
    DynError,
};
// std
use kzgrs_backend::common::blob::create_blob_idx;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{marker::PhantomData, path::PathBuf};

pub struct RocksAdapter<B, S>
where
    S: StorageSerde + Send + Sync + 'static,
{
    storage_relay: OutboundRelay<StorageMsg<RocksBackend<S>>>,
    blob: PhantomData<B>,
}

#[async_trait::async_trait]
impl<B, S> DaStorageAdapter for RocksAdapter<B, S>
where
    S: StorageSerde + Send + Sync + 'static,
    B: Blob + DeserializeOwned + Clone + Send + Sync + 'static,
    B::LightBlob: DeserializeOwned + Clone + Send + Sync + 'static,
    B::SharedCommitments: DeserializeOwned + Clone + Send + Sync + 'static,
    B::BlobId: AsRef<[u8]> + Send,
{
    type Backend = RocksBackend<S>;
    type Blob = B;
    type Settings = RocksAdapterSettings;

    async fn new(
        storage_relay: OutboundRelay<<StorageService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        Self {
            storage_relay,
            blob: PhantomData,
        }
    }

    async fn get_blob(
        &self,
        blob_id: <Self::Blob as Blob>::BlobId,
        column_idx: ColumnIndex,
    ) -> Result<Option<Self::Blob>, DynError> {
        let blob_idx = create_blob_idx(blob_id.as_ref(), column_idx.to_be_bytes().as_ref());

        let blob_prefix = format!("{}{}", DA_VERIFIED_KEY_PREFIX, DA_BLOB_PATH);
        let blob_key = key_bytes(&blob_prefix, blob_idx);

        let (blob_reply_tx, blob_reply_rx) = tokio::sync::oneshot::channel();
        self.storage_relay
            .send(StorageMsg::Load {
                key: blob_key,
                reply_channel: blob_reply_tx,
            })
            .await
            .expect("Failed to send load request to storage relay");

        let shared_commitments_prefix =
            format!("{}{}", DA_VERIFIED_KEY_PREFIX, DA_SHARED_COMMITMENTS_PATH);
        let shared_commitments_key = key_bytes(&shared_commitments_prefix, blob_id);

        let (sc_reply_tx, sc_reply_rx) = tokio::sync::oneshot::channel();

        self.storage_relay
            .send(StorageMsg::Load {
                key: shared_commitments_key,
                reply_channel: sc_reply_tx,
            })
            .await
            .expect("Failed to send load request to storage relay");

        let blob = match (blob_reply_rx.await?, sc_reply_rx.await?) {
            (Some(blob), Some(shared_commitments)) => Some(Blob::from_blob_and_shared_commitments(
                S::deserialize(blob).expect("Failed to deserialize blob"),
                S::deserialize(shared_commitments)
                    .expect("Failed to deserialize shared commitments"),
            )),
            _ => None,
        };

        Ok(blob)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RocksAdapterSettings {
    pub blob_storage_directory: PathBuf,
}
