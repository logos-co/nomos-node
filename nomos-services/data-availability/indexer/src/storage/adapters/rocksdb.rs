use std::{
    fmt::{Debug, Display},
    marker::PhantomData,
    ops::Range,
};

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
use tokio_stream::wrappers::ReceiverStream;

use crate::storage::DaStorageAdapter;

const DA_KEY_PREFIX: &str = "da";

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
    V::AppId: AsRef<[u8]> + Send + Sync,
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
        let key = meta_to_key_bytes(app_id, idx);
        let value = Bytes::from(vid.certificate_id().to_vec());

        self.storage_relay
            .send(StorageMsg::<Self::Backend>::Store { key, value })
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

            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
            let key = meta_to_key_bytes(app_id.as_ref(), current_index.as_ref());

            self.storage_relay
                .send(StorageMsg::Load {
                    key,
                    reply_channel: reply_tx,
                })
                .await
                .expect("Failed to send load request to storage relay");

            futures.push(async move {
                match reply_rx.await {
                    Ok(Some(blob)) => (idx, Some(blob)),
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

fn meta_to_key_bytes(app_id: impl AsRef<[u8]>, idx: impl AsRef<[u8]>) -> Bytes {
    let mut buffer = BytesMut::new();

    buffer.extend_from_slice(DA_KEY_PREFIX.as_bytes());
    buffer.extend_from_slice(app_id.as_ref());
    buffer.extend_from_slice(idx.as_ref());

    buffer.freeze()
}
