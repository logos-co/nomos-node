use core::{fmt::Debug, hash::Hash};

use nomos_core::{header::HeaderId, tx::Transaction};
use nomos_mempool::{
    backend::mockpool::MockPool, network::adapters::libp2p::Libp2pAdapter as MempoolNetworkAdapter,
    tx::service::openapi::Status, MempoolMetrics, MempoolMsg, TxMempoolService,
};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::{wait_with_timeout, HTTP_REQUEST_TIMEOUT};

type ClMempoolService<T> = TxMempoolService<
    MempoolNetworkAdapter<T, <T as Transaction>::Hash>,
    MockPool<HeaderId, T, <T as Transaction>::Hash>,
>;

pub async fn cl_mempool_metrics<T>(
    handle: &overwatch::overwatch::handle::OverwatchHandle,
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
    <T as nomos_core::tx::Transaction>::Hash:
        std::cmp::Ord + Debug + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static,
{
    let relay = handle.relay::<ClMempoolService<T>>().connect().await?;
    let (sender, receiver) = oneshot::channel();
    relay
        .send(MempoolMsg::Metrics {
            reply_channel: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    wait_with_timeout(
        receiver,
        HTTP_REQUEST_TIMEOUT,
        "Timeout while waiting for cl_mempool_metrics".to_string(),
    )
    .await
}

pub async fn cl_mempool_status<T>(
    handle: &overwatch::overwatch::handle::OverwatchHandle,
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
    <T as nomos_core::tx::Transaction>::Hash:
        std::cmp::Ord + Debug + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static,
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

    wait_with_timeout(
        receiver,
        HTTP_REQUEST_TIMEOUT,
        "Timeout while waiting for cl_mempool_status".to_string(),
    )
    .await
}
