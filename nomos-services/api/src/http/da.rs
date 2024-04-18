use full_replication::{AbsoluteNumber, Attestation, Blob, Certificate, FullReplication};
use nomos_core::da::blob;
use nomos_core::header::HeaderId;
use nomos_da::{
    backend::memory_cache::BlobCache, network::adapters::libp2p::Libp2pAdapter as DaNetworkAdapter,
    DaMsg, DataAvailabilityService,
};
use nomos_mempool::da::service::DaMempoolService;
use nomos_mempool::{
    backend::mockpool::MockPool, network::adapters::libp2p::Libp2pAdapter as MempoolNetworkAdapter,
    tx::service::openapi::Status,
};
use nomos_mempool::{MempoolMetrics, MempoolMsg};
use tokio::sync::oneshot;

pub type MempoolServiceDa = DaMempoolService<
    MempoolNetworkAdapter<Certificate, <Blob as blob::Blob>::Hash>,
    MockPool<HeaderId, Certificate, <Blob as blob::Blob>::Hash>,
>;

pub type DataAvailability = DataAvailabilityService<
    FullReplication<AbsoluteNumber<Attestation, Certificate>>,
    BlobCache<<Blob as nomos_core::da::blob::Blob>::Hash, Blob>,
    DaNetworkAdapter<Blob, Attestation>,
>;

pub async fn da_mempool_metrics(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
) -> Result<MempoolMetrics, super::DynError> {
    let relay = handle.relay::<MempoolServiceDa>().connect().await?;
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
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
    items: Vec<<Blob as blob::Blob>::Hash>,
) -> Result<Vec<Status<HeaderId>>, super::DynError> {
    let relay = handle.relay::<MempoolServiceDa>().connect().await?;
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
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
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
