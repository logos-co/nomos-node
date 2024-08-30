// std
use std::path::PathBuf;
use std::{marker::PhantomData, ops::Range};
// crates
use bytes::Bytes;
use futures::{stream::FuturesUnordered, Stream};
use nomos_core::da::blob::{
    info::DispersedBlobInfo,
    metadata::{Metadata, Next},
};
use nomos_core::da::BlobId;
use nomos_da_storage::fs::load_blob;
use nomos_da_storage::rocksdb::{key_bytes, DA_VERIFIED_KEY_PREFIX, DA_VID_KEY_PREFIX};
use nomos_storage::{
    backends::{rocksdb::RocksBackend, StorageSerde},
    StorageMsg, StorageService,
};
use overwatch_rs::{
    services::{relay::OutboundRelay, ServiceData},
    DynError,
};
use serde::{Deserialize, Serialize};
// internal
use crate::storage::DaStorageAdapter;

pub struct RocksAdapter<S, B>
where
    S: StorageSerde + Send + Sync + 'static,
    B: DispersedBlobInfo + Metadata + Send + Sync,
{
    settings: RocksAdapterSettings,
    storage_relay: OutboundRelay<StorageMsg<RocksBackend<S>>>,
    _vid: PhantomData<B>,
}

#[async_trait::async_trait]
impl<S, B> DaStorageAdapter for RocksAdapter<S, B>
where
    S: StorageSerde + Send + Sync + 'static,
    B: DispersedBlobInfo<BlobId = BlobId> + Metadata + Send + Sync,
    B::Index: AsRef<[u8]> + Next + Clone + PartialOrd + Send + Sync + 'static,
    B::AppId: AsRef<[u8]> + Clone + Send + Sync + 'static,
{
    type Backend = RocksBackend<S>;
    type Blob = Bytes;
    type Info = B;
    type Settings = RocksAdapterSettings;

    async fn new(
        settings: Self::Settings,
        storage_relay: OutboundRelay<<StorageService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        Self {
            settings,
            storage_relay,
            _vid: PhantomData,
        }
    }

    async fn add_index(&self, info: &Self::Info) -> Result<(), DynError> {
        let (app_id, idx) = info.metadata();

        // Check if Info in a block is something that the node've seen before.
        let attested_key = key_bytes(DA_VERIFIED_KEY_PREFIX, info.blob_id());
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        self.storage_relay
            .send(StorageMsg::LoadPrefix {
                prefix: attested_key,
                reply_channel: reply_tx,
            })
            .await
            .expect("Failed to send load request to storage relay");

        // If node haven't attested this info, return early.
        if reply_rx.await?.is_empty() {
            return Ok(());
        }

        let vid_key = key_bytes(
            DA_VID_KEY_PREFIX,
            [app_id.clone().as_ref(), idx.as_ref()].concat(),
        );

        // We are only persisting the id part of Info, the metadata can be derived from the key.
        let value = Bytes::from(info.blob_id().to_vec());

        self.storage_relay
            .send(StorageMsg::Store {
                key: vid_key,
                value,
            })
            .await
            .map_err(|(e, _)| e.into())
    }

    async fn get_range_stream(
        &self,
        app_id: <Self::Info as Metadata>::AppId,
        index_range: Range<<Self::Info as Metadata>::Index>,
    ) -> Box<dyn Stream<Item = (<Self::Info as Metadata>::Index, Vec<Bytes>)> + Unpin + Send> {
        let futures = FuturesUnordered::new();

        // TODO: Using while loop here until `Step` trait is stable.
        //
        // For index_range to be used as Range with the stepping capabilities (eg. `for idx in
        // item_range`), Metadata::Index needs to implement `Step` trait, which is unstable.
        // See issue #42168 <https://github.com/rust-lang/rust/issues/42168> for more information.
        let mut current_index = index_range.start.clone();
        while current_index <= index_range.end {
            let idx = current_index.clone();
            let app_id = app_id.clone();
            let settings = self.settings.clone();

            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
            let key = key_bytes(
                DA_VID_KEY_PREFIX,
                [app_id.as_ref(), current_index.as_ref()].concat(),
            );

            self.storage_relay
                .send(StorageMsg::Load {
                    key,
                    reply_channel: reply_tx,
                })
                .await
                .expect("Failed to send load request to storage relay");

            futures.push(async move {
                match reply_rx.await {
                    Ok(Some(id)) => (idx, load_blob(settings.blob_storage_directory, &id).await),
                    Ok(None) => (idx, Vec::new()),
                    Err(_) => {
                        tracing::error!("Failed to receive storage response");
                        (idx, Vec::new())
                    }
                }
            });

            current_index = current_index.next();
        }

        Box::new(futures)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RocksAdapterSettings {
    pub blob_storage_directory: PathBuf,
}
