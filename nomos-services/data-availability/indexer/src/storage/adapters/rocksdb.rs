use std::{marker::PhantomData, ops::Range};

use bytes::{Bytes, BytesMut};

use futures::{stream::FuturesUnordered, Stream};
use nomos_core::da::certificate::{
    metadata::{Metadata, Next},
    vid::VID,
};
use nomos_storage::{
    backends::{rocksdb::RocksBackend, StorageSerde},
    StorageMsg, StorageService,
};
use overwatch_rs::{
    services::{relay::OutboundRelay, ServiceData},
    DynError,
};
use tokio::{fs::File, io::AsyncReadExt};

use crate::storage::DaStorageAdapter;

const DA_VID_KEY_PREFIX: &str = "da/vid/";
const DA_ATTESTED_KEY_PREFIX: &str = "da/attested/";
const DA_BLOB_PATH: &str = "blobs/";

pub struct RocksAdapter<S, V>
where
    S: StorageSerde + Send + Sync + 'static,
    V: VID + Metadata + Send + Sync,
{
    storage_relay: OutboundRelay<StorageMsg<RocksBackend<S>>>,
    _vid: PhantomData<V>,
}

#[async_trait::async_trait]
impl<S, V> DaStorageAdapter for RocksAdapter<S, V>
where
    S: StorageSerde + Send + Sync + 'static,
    V: VID<CertificateId = [u8; 32]> + Metadata + Send + Sync,
    V::Index: AsRef<[u8]> + Next + Clone + PartialOrd + Send + Sync + 'static,
    V::AppId: AsRef<[u8]> + Clone + Send + Sync + 'static,
{
    type Backend = RocksBackend<S>;
    type Blob = Bytes;
    type VID = V;

    async fn new(
        storage_relay: OutboundRelay<<StorageService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        Self {
            storage_relay,
            _vid: PhantomData,
        }
    }

    async fn add_index(&self, vid: &Self::VID) -> Result<(), DynError> {
        let (app_id, idx) = vid.metadata();

        // Check if VID in a block is something that the node've seen before.
        let attested_key = meta_to_key_bytes(DA_ATTESTED_KEY_PREFIX, vid.certificate_id());
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        // Remove item from attested list as it shouldn't be used again.
        self.storage_relay
            .send(StorageMsg::Remove {
                key: attested_key,
                reply_channel: reply_tx,
            })
            .await
            .expect("Failed to send load request to storage relay");

        // If node haven't attested this vid, return early.
        if reply_rx.await?.is_none() {
            return Ok(());
        }

        let vid_key = meta_to_key_bytes(
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

            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
            let key = meta_to_key_bytes(
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
                    Ok(Some(id)) => (idx, Some(load_blob(app_id.as_ref(), &id).await)),
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

fn meta_to_key_bytes(prefix: &str, id: impl AsRef<[u8]>) -> Bytes {
    let mut buffer = BytesMut::new();

    buffer.extend_from_slice(prefix.as_bytes());
    buffer.extend_from_slice(id.as_ref());

    buffer.freeze()
}

// TODO: Rocksdb has a feature called BlobDB that handles largo blob storing, but further
// investigation needs to be done to see if rust wrapper supports it.
async fn load_blob(app_id: &[u8], id: &[u8]) -> Bytes {
    let app_id = hex::encode(app_id);
    let id = hex::encode(id);
    let path = format!("{DA_BLOB_PATH}/{app_id}/{id}");

    let mut file = match File::open(path).await {
        Ok(file) => file,
        Err(e) => {
            eprintln!("Failed to open file: {}", e);
            return Bytes::new();
        }
    };

    let mut contents = vec![];
    if let Err(e) = file.read_to_end(&mut contents).await {
        tracing::error!("Failed to read file: {}", e);
        return Bytes::new();
    }

    Bytes::from(contents)
}
