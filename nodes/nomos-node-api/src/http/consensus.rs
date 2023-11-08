use consensus_engine::{Block, BlockId};
use nomos_consensus::{CarnotInfo, ConsensusMsg};
use nomos_node_lib::Carnot;

use overwatch_rs::overwatch::handle::OverwatchHandle;
use tokio::sync::oneshot;

pub async fn carnot_info(handle: &OverwatchHandle) -> Result<CarnotInfo, overwatch_rs::DynError> {
    let relay = handle.relay::<Carnot>().connect().await?;
    let (sender, receiver) = oneshot::channel();
    relay
        .send(ConsensusMsg::Info { tx: sender })
        .await
        .map_err(|(e, _)| e)?;

    Ok(receiver.await?)
}

pub async fn carnot_blocks(
    handle: &OverwatchHandle,
    from: Option<BlockId>,
    to: Option<BlockId>,
) -> Result<Vec<Block>, super::DynError> {
    let relay = handle.relay::<Carnot>().connect().await?;
    let (sender, receiver) = oneshot::channel();
    relay
        .send(ConsensusMsg::GetBlocks {
            from,
            to,
            tx: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    Ok(receiver.await?)
}
