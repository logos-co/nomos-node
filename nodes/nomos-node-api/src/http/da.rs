use full_replication::Blob;
use nomos_core::da::blob;
use nomos_da::DaMsg;
use nomos_mempool::{
    openapi::{MempoolMetrics, Status},
    MempoolMsg,
};
use nomos_node_types::{DaMempoolService, DataAvailability};
use overwatch_rs::overwatch::handle::OverwatchHandle;
use tokio::sync::oneshot;

pub async fn da_mempool_metrics(
    handle: &OverwatchHandle,
) -> Result<MempoolMetrics, super::DynError> {
    let relay = handle.relay::<DaMempoolService>().connect().await?;
    let (sender, receiver) = oneshot::channel();
    relay
        .send(MempoolMsg::Metrics {
            reply_channel: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    Ok(receiver.await.unwrap())
}

pub async fn da_mempool_status(
    handle: &OverwatchHandle,
    items: Vec<<Blob as blob::Blob>::Hash>,
) -> Result<Vec<Status>, super::DynError> {
    let relay = handle.relay::<DaMempoolService>().connect().await?;
    let (sender, receiver) = oneshot::channel();
    relay
        .send(MempoolMsg::Status {
            items,
            reply_channel: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    Ok(receiver.await.unwrap())
}

pub async fn da_blobs(
    handle: &OverwatchHandle,
    ids: Vec<<Blob as blob::Blob>::Hash>,
) -> Result<Vec<Blob>, super::DynError> {
    let relay = handle.relay::<DataAvailability>().connect().await?;
    let (reply_channel, receiver) = oneshot::channel();
    relay
        .send(DaMsg::Get {
            ids: Box::new(ids.into_iter()),
            reply_channel,
        })
        .await
        .map_err(|(e, _)| e)?;

    Ok(receiver.await?)
}
