use core::{fmt::Debug, hash::Hash};

use nomos_core::header::HeaderId;
use nomos_core::tx::Transaction;
use nomos_mempool::{
    backend::mockpool::MockPool, network::adapters::libp2p::Libp2pAdapter as MempoolNetworkAdapter,
    tx::service::openapi::Status, MempoolMetrics, MempoolMsg, TxMempoolService,
};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

type ClMempoolService<T> = TxMempoolService<
    MempoolNetworkAdapter<T, <T as Transaction>::Hash>,
    MockPool<HeaderId, T, <T as Transaction>::Hash>,
>;

pub async fn cl_mempool_metrics<T>(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
) -> Result<MempoolMetrics, super::DynError>
where
    T: Transaction
        + Clone
        + Debug
        + Hash
        + Serialize
        + for<'de> Deserialize<'de>
        + Send
        + Sync
        + 'static,
    <T as nomos_core::tx::Transaction>::Hash: std::cmp::Ord + Debug + Send + Sync + 'static,
{
    let relay = handle.relay::<ClMempoolService<T>>().connect().await?;
    let (sender, receiver) = oneshot::channel();
    relay
        .send(MempoolMsg::Metrics {
            reply_channel: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    Ok(receiver.await?)
}

pub async fn cl_mempool_status<T>(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
    items: Vec<<T as Transaction>::Hash>,
) -> Result<Vec<Status<HeaderId>>, super::DynError>
where
    T: Transaction
        + Clone
        + Debug
        + Hash
        + Serialize
        + for<'de> Deserialize<'de>
        + Send
        + Sync
        + 'static,
    <T as nomos_core::tx::Transaction>::Hash: std::cmp::Ord + Debug + Send + Sync + 'static,
{
    let relay = handle.relay::<ClMempoolService<T>>().connect().await?;
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
