// std
// crates
use nomos_consensus::{CarnotInfo, ConsensusMsg};

use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
use tracing::error;
// internal
use nomos_http::backends::axum::AxumBackend;
use nomos_http::bridge::{build_http_bridge, HttpBridgeRunner};
use nomos_http::http::{HttpMethod, HttpRequest, HttpResponse};
use nomos_mempool::backend::mockpool::MockPool;

use nomos_mempool::{MempoolMetrics, MempoolMsg, MempoolService};

#[cfg(feature = "libp2p")]
use nomos_mempool::network::adapters::libp2p::Libp2pAdapter;
#[cfg(feature = "waku")]
use nomos_mempool::network::adapters::waku::WakuAdapter;
#[cfg(feature = "libp2p")]
use nomos_network::backends::libp2p::Libp2p;
#[cfg(feature = "waku")]
use nomos_network::backends::waku::Waku;
use nomos_network::NetworkService;
use nomos_node::{Carnot, Tx};
use overwatch_rs::services::relay::OutboundRelay;

#[cfg(feature = "waku")]
mod waku;
#[cfg(feature = "waku")]
use waku::*;
#[cfg(feature = "libp2p")]
mod libp2p;
#[cfg(feature = "libp2p")]
use libp2p::*;

macro_rules! get_handler {
    ($handle:expr, $service:ty, $path:expr => $handler:tt) => {{
        let (channel, mut http_request_channel) =
            build_http_bridge::<$service, AxumBackend, _>($handle, HttpMethod::GET, $path)
                .await
                .unwrap();
        while let Some(HttpRequest { res_tx, .. }) = http_request_channel.recv().await {
            if let Err(e) = $handler(&channel, res_tx).await {
                error!(e);
            }
        }
        Ok(())
    }};
}

pub fn carnot_info_bridge(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner {
    Box::new(Box::pin(async move {
        get_handler!(handle, Carnot, "info" => handle_carnot_info_req)
    }))
}

pub fn mempool_metrics_bridge(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner {
    Box::new(Box::pin(async move {
        #[cfg(feature = "waku")]
        {
            get_handler!(handle, MempoolService<WakuAdapter<Tx>, MockPool<Tx>>, "metrics" => handle_mempool_metrics_req)
        }
        #[cfg(feature = "libp2p")]
        get_handler!(handle, MempoolService<Libp2pAdapter<Tx>, MockPool<Tx>>, "metrics" => handle_mempool_metrics_req)
    }))
}

pub fn network_info_bridge(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner {
    Box::new(Box::pin(async move {
        #[cfg(feature = "waku")]
        {
            get_handler!(handle, NetworkService<Waku>, "info" => handle_waku_info_req)
        }
        #[cfg(feature = "libp2p")]
        get_handler!(handle, NetworkService<Libp2p>, "info" => handle_libp2p_info_req)
    }))
}

#[cfg(feature = "waku")]
pub fn mempool_add_tx_bridge(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner {
    Box::new(Box::pin(async move {
        let (mempool_channel, mut http_request_channel) =
            build_http_bridge::<MempoolService<WakuAdapter<Tx>, MockPool<Tx>>, AxumBackend, _>(
                handle.clone(),
                HttpMethod::POST,
                "addtx",
            )
            .await
            .unwrap();

        while let Some(HttpRequest {
            res_tx, payload, ..
        }) = http_request_channel.recv().await
        {
            if let Err(e) =
                handle_mempool_add_tx_req(&handle, &mempool_channel, res_tx, payload).await
            {
                error!(e);
            }
        }
        Ok(())
    }))
}

#[cfg(feature = "waku")]
pub fn waku_add_conn_bridge(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner {
    Box::new(Box::pin(async move {
        let (waku_channel, mut http_request_channel) = build_http_bridge::<
            NetworkService<Waku>,
            AxumBackend,
            _,
        >(handle, HttpMethod::POST, "conn")
        .await
        .unwrap();

        while let Some(HttpRequest {
            res_tx, payload, ..
        }) = http_request_channel.recv().await
        {
            if let Err(e) = handle_add_conn_req(&waku_channel, res_tx, payload).await {
                error!(e);
            }
        }
        Ok(())
    }))
}

async fn handle_carnot_info_req(
    carnot_channel: &OutboundRelay<ConsensusMsg>,
    res_tx: Sender<HttpResponse>,
) -> Result<(), overwatch_rs::DynError> {
    let (sender, receiver) = oneshot::channel();
    carnot_channel
        .send(ConsensusMsg::Info { tx: sender })
        .await
        .map_err(|(e, _)| e)?;
    let carnot_info: CarnotInfo = receiver.await.unwrap();
    res_tx
        .send(Ok(serde_json::to_vec(&carnot_info)?.into()))
        .await?;

    Ok(())
}

async fn handle_mempool_metrics_req(
    mempool_channel: &OutboundRelay<MempoolMsg<Tx>>,
    res_tx: Sender<HttpResponse>,
) -> Result<(), overwatch_rs::DynError> {
    let (sender, receiver) = oneshot::channel();
    mempool_channel
        .send(MempoolMsg::Metrics {
            reply_channel: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    let metrics: MempoolMetrics = receiver.await.unwrap();
    res_tx
        // TODO: use serde to serialize metrics
        .send(Ok(format!(
            "{{\"pending_tx\": {}, \"last_tx\": {}}}",
            metrics.pending_txs, metrics.last_tx_timestamp
        )
        .into()))
        .await?;

    Ok(())
}
