use std::{marker::PhantomData, path::PathBuf};

use kzgrs_backend::common::ColumnIndex;
use nomos_core::da::blob::Blob;
use nomos_da_storage::rocksdb::{
    create_blob_idx, key_bytes, DA_BLOB_PREFIX, DA_SHARED_COMMITMENTS_PREFIX,
};
use nomos_storage::{
    backends::{rocksdb::RocksBackend, StorageSerde},
    StorageMsg, StorageService,
};
use overwatch_rs::{
    services::{relay::OutboundRelay, ServiceData},
    DynError,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::storage::DaStorageAdapter;

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

    async fn get_commitments(
        &self,
        blob_id: <Self::Blob as Blob>::BlobId,
    ) -> Result<Option<<Self::Blob as Blob>::SharedCommitments>, DynError> {
        let shared_commitments_key = key_bytes(DA_SHARED_COMMITMENTS_PREFIX, blob_id);
        let (sc_reply_tx, sc_reply_rx) = tokio::sync::oneshot::channel();
        self.storage_relay
            .send(StorageMsg::Load {
                key: shared_commitments_key,
                reply_channel: sc_reply_tx,
            })
            .await
            .expect("Failed to send load request to storage relay");

        let shared_commitments = sc_reply_rx.await?;
        let shared_commitments = shared_commitments
            .map(|sc| S::deserialize(sc).expect("Failed to deserialize shared commitments"));

        Ok(shared_commitments)
    }

    async fn get_light_blob(
        &self,
        blob_id: <Self::Blob as Blob>::BlobId,
        column_idx: ColumnIndex,
    ) -> Result<Option<<Self::Blob as Blob>::LightBlob>, DynError> {
        let blob_idx = create_blob_idx(blob_id.as_ref(), column_idx.to_be_bytes().as_ref());
        let blob_key = key_bytes(DA_BLOB_PREFIX, blob_idx);
        let (blob_reply_tx, blob_reply_rx) = tokio::sync::oneshot::channel();
        self.storage_relay
            .send(StorageMsg::Load {
                key: blob_key,
                reply_channel: blob_reply_tx,
            })
            .await
            .expect("Failed to send load request to storage relay");

        let blob = blob_reply_rx.await?;
        let blob = blob.map(|blob| S::deserialize(blob).expect("Failed to deserialize blob"));

        Ok(blob)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RocksAdapterSettings {
    pub blob_storage_directory: PathBuf,
}
