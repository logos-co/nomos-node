use nomos_consensus::{CarnotInfo, ConsensusMsg};
use nomos_node_lib::Carnot;

use tokio::sync::oneshot;

pub async fn carnot_info(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
) -> Result<CarnotInfo, overwatch_rs::DynError> {
    let relay = handle.relay::<Carnot>().connect().await?;
    let (sender, receiver) = oneshot::channel();
    relay
        .send(ConsensusMsg::Info { tx: sender })
        .await
        .map_err(|(e, _)| e)?;
    Ok(receiver.await?)
}
