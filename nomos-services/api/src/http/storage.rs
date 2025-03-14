use std::{hash::Hash, sync::Arc};

use nomos_core::{
    block::Block,
    da::blob::{Blob, LightBlob},
    header::HeaderId,
};
use nomos_da_storage::rocksdb::{
    create_blob_idx, key_bytes, DA_BLOB_PREFIX, DA_SHARED_COMMITMENTS_PREFIX,
};
use nomos_libp2p::libp2p::{
    bytes::Bytes,
    futures::{stream, Stream, StreamExt},
};
use nomos_storage::{
    backends::{rocksdb::RocksBackend, StorageSerde},
    StorageMsg, StorageService,
};
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::oneshot;

use crate::http::DynError;

pub async fn block_req<S, Tx>(
    handle: &overwatch::overwatch::handle::OverwatchHandle,
    id: HeaderId,
) -> Result<Option<Block<Tx, kzgrs_backend::dispersal::BlobInfo>>, super::DynError>
where
    Tx: serde::Serialize + DeserializeOwned + Clone + Eq + core::hash::Hash,
    S: StorageSerde + Send + Sync + 'static,
{
    let relay = handle
        .relay::<StorageService<RocksBackend<S>>>()
        .connect()
        .await?;
    let (msg, receiver) = StorageMsg::new_load_message(id);
    relay.send(msg).await.map_err(|(e, _)| e)?;

    Ok(receiver.recv().await?)
}

pub async fn get_shared_commitments<StorageOp, DaBlob>(
    handle: &overwatch::overwatch::handle::OverwatchHandle,
    blob_id: <DaBlob as Blob>::BlobId,
) -> Result<Option<<DaBlob as Blob>::SharedCommitments>, super::DynError>
where
    StorageOp: StorageSerde + Send + Sync + 'static,
    DaBlob: Blob,
    <DaBlob as Blob>::BlobId:
        AsRef<[u8]> + serde::Serialize + DeserializeOwned + Send + Sync + 'static,
    <DaBlob as Blob>::SharedCommitments:
        serde::Serialize + DeserializeOwned + Send + Sync + 'static,
    <StorageOp as StorageSerde>::Error: Send + Sync,
{
    let relay = handle
        .relay::<StorageService<RocksBackend<StorageOp>>>()
        .connect()
        .await?;

    let commitments_id = key_bytes(DA_SHARED_COMMITMENTS_PREFIX, blob_id.as_ref());
    let (reply_tx, reply_rcv) = tokio::sync::oneshot::channel();
    relay
        .send(StorageMsg::Load {
            key: commitments_id,
            reply_channel: reply_tx,
        })
        .await
        .map_err(|(e, _)| e)?;

    let result = reply_rcv.await?;
    result
        .map(|data| StorageOp::deserialize(data))
        .transpose()
        .map_err(super::DynError::from)
}

pub async fn get_light_blob<StorageOp, DaBlob>(
    handle: &overwatch::overwatch::handle::OverwatchHandle,
    blob_id: <DaBlob as Blob>::BlobId,
    column_idx: <DaBlob as Blob>::ColumnIndex,
) -> Result<Option<<DaBlob as Blob>::LightBlob>, super::DynError>
where
    StorageOp: StorageSerde + Send + Sync + 'static,
    DaBlob: Blob,
    <DaBlob as Blob>::BlobId: AsRef<[u8]> + DeserializeOwned + Send + Sync + 'static,
    <DaBlob as Blob>::ColumnIndex: AsRef<[u8]> + DeserializeOwned + Send + Sync + 'static,
    <DaBlob as Blob>::LightBlob: LightBlob + Serialize + DeserializeOwned,
    <StorageOp as StorageSerde>::Error: Send + Sync,
{
    let relay = handle
        .relay::<StorageService<RocksBackend<StorageOp>>>()
        .connect()
        .await?;

    let blob_idx = create_blob_idx(blob_id.as_ref(), column_idx.as_ref());
    let blob_key = key_bytes(DA_BLOB_PREFIX, blob_idx);
    let (reply_tx, reply_rcv) = tokio::sync::oneshot::channel();
    relay
        .send(StorageMsg::Load {
            key: blob_key,
            reply_channel: reply_tx,
        })
        .await
        .map_err(|(e, _)| e)?;

    let result = reply_rcv.await?;
    result
        .map(|data| StorageOp::deserialize(data))
        .transpose()
        .map_err(super::DynError::from)
}

pub async fn get_shares<StorageOp, DaBlob>(
    handle: &overwatch::overwatch::handle::OverwatchHandle,
    blob_id: DaBlob::BlobId,
    requested_shares: Vec<DaBlob::ColumnIndex>,
    filter_shares: Vec<DaBlob::ColumnIndex>,
    return_available: bool,
) -> Result<impl Stream<Item = Result<Bytes, DynError>> + Send + Sync + 'static, DynError>
where
    StorageOp: StorageSerde + Send + Sync + 'static,
    DaBlob: Blob,
    DaBlob::BlobId: AsRef<[u8]> + DeserializeOwned + Send + Sync + 'static,
    DaBlob::ColumnIndex: DeserializeOwned + Hash + Eq + Send + Sync + 'static,
    DaBlob::LightBlob: LightBlob<ColumnIndex = <DaBlob as Blob>::ColumnIndex>
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    StorageOp::Error: Send + Sync,
{
    let storage_relay = handle
        .relay::<StorageService<RocksBackend<StorageOp>>>()
        .connect()
        .await?;

    let shares_prefix = key_bytes(DA_BLOB_PREFIX, blob_id.as_ref());

    let (blob_reply_tx, blob_reply_rx) = oneshot::channel();
    storage_relay
        .send(StorageMsg::LoadPrefix {
            prefix: shares_prefix,
            reply_channel: blob_reply_tx,
        })
        .await
        .map_err(|(e, _)| e)?;

    let blobs = blob_reply_rx.await?;

    let requested_shares = Arc::new(requested_shares);
    let filter_shares = Arc::new(filter_shares);

    // Wrapping into stream from here because currently storage backend returns
    // collection
    Ok(stream::iter(blobs)
        .filter_map(|bytes| async move { StorageOp::deserialize::<DaBlob::LightBlob>(bytes).ok() })
        .filter_map(move |blob| {
            let requested = requested_shares.clone();
            let filtered = filter_shares.clone();
            async move {
                if requested.contains(&blob.column_idx())
                    || (return_available && !filtered.contains(&blob.column_idx()))
                {
                    let Ok(mut json) = serde_json::to_vec(&blob) else {
                        return None;
                    };
                    json.push(b'\n');
                    Some(Ok(Bytes::from(json)))
                } else {
                    None
                }
            }
        }))
}
