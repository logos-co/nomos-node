use nomos_core::tx::Transaction;
use nomos_mempool::{
    openapi::{MempoolMetrics, Status},
    MempoolMsg,
};
use nomos_node_types::{tx::Tx, ClMempoolService};
use tokio::sync::oneshot;

pub async fn cl_mempool_metrics(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
) -> Result<MempoolMetrics, super::DynError> {
    let relay = handle.relay::<ClMempoolService>().connect().await?;
    let (sender, receiver) = oneshot::channel();
    relay
        .send(MempoolMsg::Metrics {
            reply_channel: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    Ok(receiver.await?)
}

pub async fn cl_mempool_status(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
    items: Vec<<Tx as Transaction>::Hash>,
) -> Result<Vec<Status>, super::DynError> {
    let relay = handle.relay::<ClMempoolService>().connect().await?;
    let (sender, receiver) = oneshot::channel();
    relay
        .send(MempoolMsg::Status {
            items,
            reply_channel: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    Ok(receiver.await?)
}
