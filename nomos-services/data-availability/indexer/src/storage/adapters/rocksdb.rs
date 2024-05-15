use std::path::PathBuf;
use std::{marker::PhantomData, ops::Range};

use bytes::Bytes;
use futures::{stream::FuturesUnordered, Stream};
use nomos_core::da::certificate::{
    metadata::{Metadata, Next},
    vid::VidCertificate,
};
use nomos_da_storage::fs::load_blob;
use nomos_da_storage::rocksdb::{key_bytes, DA_ATTESTED_KEY_PREFIX, DA_VID_KEY_PREFIX};
use nomos_storage::{
    backends::{rocksdb::RocksBackend, StorageSerde},
    StorageMsg, StorageService,
};
use overwatch_rs::{
    services::{relay::OutboundRelay, ServiceData},
    DynError,
};

use crate::storage::DaStorageAdapter;

pub struct RocksAdapter<S, V>
where
    S: StorageSerde + Send + Sync + 'static,
    V: VidCertificate + Metadata + Send + Sync,
{
    settings: RocksAdapterSettings,
    storage_relay: OutboundRelay<StorageMsg<RocksBackend<S>>>,
    _vid: PhantomData<V>,
}

#[async_trait::async_trait]
impl<S, V> DaStorageAdapter for RocksAdapter<S, V>
where
    S: StorageSerde + Send + Sync + 'static,
    V: VidCertificate<CertificateId = [u8; 32]> + Metadata + Send + Sync,
    V::Index: AsRef<[u8]> + Next + Clone + PartialOrd + Send + Sync + 'static,
    V::AppId: AsRef<[u8]> + Clone + Send + Sync + 'static,
{
    type Backend = RocksBackend<S>;
    type Blob = Bytes;
    type VID = V;
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

    async fn add_index(&self, vid: &Self::VID) -> Result<(), DynError> {
        let (app_id, idx) = vid.metadata();

        // Check if VID in a block is something that the node've seen before.
        let attested_key = key_bytes(DA_ATTESTED_KEY_PREFIX, vid.certificate_id());
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        // Remove item from attested list as it shouldn't be used again.
        self.storage_relay
            .send(StorageMsg::Load {
                key: attested_key,
                reply_channel: reply_tx,
            })
            .await
            .expect("Failed to send load request to storage relay");

        // If node haven't attested this vid, return early.
        if reply_rx.await?.is_none() {
            return Ok(());
        }

        let vid_key = key_bytes(
            DA_VID_KEY_PREFIX,
            [app_id.clone().as_ref(), idx.as_ref()].concat(),
        );

        // We are only persisting the id part of VID, the metadata can be derived from the key.
        let value = Bytes::from(vid.certificate_id().to_vec());

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
        app_id: <Self::VID as Metadata>::AppId,
        index_range: Range<<Self::VID as Metadata>::Index>,
    ) -> Box<dyn Stream<Item = (<Self::VID as Metadata>::Index, Option<Bytes>)> + Unpin + Send>
    {
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
                    Ok(None) => (idx, None),
                    Err(_) => {
                        tracing::error!("Failed to receive storage response");
                        (idx, None)
                    }
                }
            });

            current_index = current_index.next();
        }

        Box::new(futures)
    }
}

#[derive(Debug, Clone)]
pub struct RocksAdapterSettings {
    pub blob_storage_directory: PathBuf,
}
