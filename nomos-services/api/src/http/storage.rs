use nomos_core::block::Block;
use nomos_core::header::HeaderId;
use nomos_storage::{
    backends::{rocksdb::RocksBackend, StorageSerde},
    StorageMsg, StorageService,
};

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
