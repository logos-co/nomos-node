use consensus_engine::BlockId;
use nomos_core::block::Block;
use nomos_storage::{
    backends::{sled::SledBackend, StorageSerde},
    StorageMsg, StorageService,
};

pub async fn block_req<S, Tx>(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
    id: BlockId,
) -> Result<Option<Block<Tx, full_replication::Certificate>>, super::DynError>
where
    Tx: serde::Serialize + serde::de::DeserializeOwned + Clone + Eq + core::hash::Hash,
    S: StorageSerde + Send + Sync + 'static,
{
    let relay = handle
        .relay::<StorageService<SledBackend<S>>>()
        .connect()
        .await?;
    let (msg, receiver) = StorageMsg::new_load_message(id);
    relay.send(msg).await.map_err(|(e, _)| e)?;

    Ok(receiver.recv().await?)
}
