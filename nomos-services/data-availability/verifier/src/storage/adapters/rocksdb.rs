// std
use serde::{de::DeserializeOwned, Serialize};
use std::{marker::PhantomData, path::PathBuf};
// crates
use nomos_core::da::{attestation::Attestation, blob::Blob};
use nomos_da_storage::{
    fs::write_blob,
    rocksdb::{key_bytes, DA_ATTESTED_KEY_PREFIX},
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
    A: Attestation + Serialize + DeserializeOwned + Clone + Send + Sync,
    B: Blob + AsRef<[u8]> + Clone + Send + Sync + 'static,
    B::BlobId: AsRef<[u8]> + Send + Sync + 'static,
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
        write_blob(
            self.settings.blob_storage_directory.clone(),
            blob.id().as_ref(),
            blob.as_ref(),
        )
        .await?;

        // Mark blob as attested for lateer use in Indexer and attestation cache.
        let blob_key = key_bytes(DA_ATTESTED_KEY_PREFIX, blob.id().as_ref());
        self.storage_relay
            .send(StorageMsg::Store {
                key: blob_key,
                value: S::serialize(attestation),
            })
            .await
            .map_err(|(e, _)| e.into())
    }

    async fn get_attestation(
        &self,
        blob: &Self::Blob,
    ) -> Result<Option<Self::Attestation>, DynError> {
        let attestation_key = key_bytes(DA_ATTESTED_KEY_PREFIX, blob.id().as_ref());
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.storage_relay
            .send(StorageMsg::Load {
                key: attestation_key,
                reply_channel: reply_tx,
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
                    S::deserialize(bytes).expect("Attestation should be deserialized from bytes")
                })
            })
            .map_err(|_| "".into())
    }
}

#[derive(Debug, Clone)]
pub struct RocksAdapterSettings {
    pub blob_storage_directory: PathBuf,
}
