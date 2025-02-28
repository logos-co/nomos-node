use nomos_core::da::blob::Blob;
use nomos_da_storage::rocksdb::{key_bytes, DA_SHARED_COMMITMENTS_PREFIX};
use nomos_core::{block::Block, header::HeaderId};
use nomos_storage::{
    backends::{rocksdb::RocksBackend, StorageSerde},
    StorageMsg, StorageService,
};
use serde::de::DeserializeOwned;

pub async fn block_req<S, Tx>(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
    id: HeaderId,
) -> Result<Option<Block<Tx, full_replication::BlobInfo>>, super::DynError>
where
    Tx: serde::Serialize + serde::de::DeserializeOwned + Clone + Eq + core::hash::Hash,
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
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
    blob_id: <DaBlob as Blob>::BlobId,
) -> Result<Option<<DaBlob as Blob>::SharedCommitments>, super::DynError>
where
    StorageOp: StorageSerde + Send + Sync + 'static,
    DaBlob: Blob + serde::Serialize + DeserializeOwned + Send + Sync + 'static,
    <DaBlob as Blob>::BlobId:
        AsRef<[u8]> + serde::Serialize + DeserializeOwned + Send + Sync + 'static,
    <DaBlob as Blob>::SharedCommitments:
        serde::Serialize + DeserializeOwned + Send + Sync + 'static,
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

    let result = reply_rcv.await.unwrap();
    let option = result.map(|data| StorageOp::deserialize(data).unwrap());
    Ok(option)
}