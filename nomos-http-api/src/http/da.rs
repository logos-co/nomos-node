use full_replication::{Blob, Certificate};
use nomos_core::da::blob;
use nomos_mempool::{
    backend::mockpool::MockPool,
    network::adapters::libp2p::Libp2pAdapter,
    openapi::{MempoolMetrics, Status},
    Certificate as CertDiscriminant, MempoolMsg, MempoolService,
};
use tokio::sync::oneshot;

pub(crate) type DaMempoolService = MempoolService<
    Libp2pAdapter<Certificate, <Blob as blob::Blob>::Hash>,
    MockPool<Certificate, <Blob as blob::Blob>::Hash>,
    CertDiscriminant,
>;

pub(crate) async fn da_mempool_metrics(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
) -> Result<MempoolMetrics, super::DynError> {
    let relay = handle.relay::<DaMempoolService>().connect().await.unwrap();
    let (sender, receiver) = oneshot::channel();
    relay
        .send(MempoolMsg::Metrics {
            reply_channel: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    Ok(receiver.await.unwrap())
}

pub(crate) async fn da_mempool_status(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
    items: Vec<<Blob as blob::Blob>::Hash>,
) -> Result<Vec<Status>, super::DynError> {
    let relay = handle.relay::<DaMempoolService>().connect().await.unwrap();
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
