use consensus_engine::BlockId;
use nomos_core::block::Block;
use nomos_node_lib::{tx::Tx, Wire};
use nomos_storage::{backends::sled::SledBackend, StorageMsg, StorageService};

pub async fn block_req(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
    id: BlockId,
) -> Result<Option<Block<Tx, full_replication::Certificate>>, super::DynError> {
    let relay = handle
        .relay::<StorageService<SledBackend<Wire>>>()
        .connect()
        .await?;
    let (msg, receiver) = StorageMsg::new_load_message(id);
    relay.send(msg).await.map_err(|(e, _)| e)?;

    Ok(receiver.recv().await?)
}
