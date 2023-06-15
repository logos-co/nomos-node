// std
// crates
use crate::Carnot;
use bytes::Bytes;
use http::StatusCode;
use nomos_consensus::{CarnotInfo, ConsensusMsg};
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
use tracing::error;
// internal
use crate::tx::Tx;
use futures::future::join_all;
use multiaddr::Multiaddr;
use nomos_core::wire;
use nomos_http::backends::axum::AxumBackend;
use nomos_http::bridge::{build_http_bridge, HttpBridgeRunner};
use nomos_http::http::{HttpMethod, HttpRequest, HttpResponse};
use nomos_mempool::backend::mockpool::MockPool;
use nomos_mempool::network::adapters::waku::{
    WakuAdapter, WAKU_CARNOT_PUB_SUB_TOPIC, WAKU_CARNOT_TX_CONTENT_TOPIC,
};
use nomos_mempool::{MempoolMetrics, MempoolMsg, MempoolService};
use nomos_network::backends::waku::{Waku, WakuBackendMessage, WakuInfo};
use nomos_network::{NetworkMsg, NetworkService};
use overwatch_rs::services::relay::OutboundRelay;
use waku_bindings::WakuMessage;

pub fn carnot_info_bridge(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner {
    Box::new(Box::pin(async move {
        let (carnot_channel, mut http_request_channel) =
            build_http_bridge::<Carnot, AxumBackend, _>(handle, HttpMethod::GET, "info")
                .await
                .unwrap();

        while let Some(HttpRequest { res_tx, .. }) = http_request_channel.recv().await {
            if let Err(e) = handle_carnot_info_req(&carnot_channel, &res_tx).await {
                error!(e);
            }
        }

        Ok(())
    }))
}

pub fn mempool_metrics_bridge(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner {
    Box::new(Box::pin(async move {
        let (mempool_channel, mut http_request_channel) =
            build_http_bridge::<MempoolService<WakuAdapter<Tx>, MockPool<Tx>>, AxumBackend, _>(
                handle,
                HttpMethod::GET,
                "metrics",
            )
            .await
            .unwrap();

        while let Some(HttpRequest { res_tx, .. }) = http_request_channel.recv().await {
            if let Err(e) = handle_mempool_metrics_req(&mempool_channel, res_tx).await {
                error!(e);
            }
        }
        Ok(())
    }))
}

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

pub fn waku_info_bridge(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner {
    Box::new(Box::pin(async move {
        let (waku_channel, mut http_request_channel) = build_http_bridge::<
            NetworkService<Waku>,
            AxumBackend,
            _,
        >(handle, HttpMethod::GET, "info")
        .await
        .unwrap();

        while let Some(HttpRequest { res_tx, .. }) = http_request_channel.recv().await {
            if let Err(e) = handle_waku_info_req(&waku_channel, &res_tx).await {
                error!(e);
            }
        }
        Ok(())
    }))
}

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
    res_tx: &Sender<HttpResponse>,
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

async fn handle_mempool_add_tx_req(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
    mempool_channel: &OutboundRelay<MempoolMsg<Tx>>,
    res_tx: Sender<HttpResponse>,
    payload: Option<Bytes>,
) -> Result<(), overwatch_rs::DynError> {
    if let Some(data) = payload
        .as_ref()
        .and_then(|b| String::from_utf8(b.to_vec()).ok())
    {
        let tx = Tx(data);
        let (sender, receiver) = oneshot::channel();
        mempool_channel
            .send(MempoolMsg::AddTx {
                tx: tx.clone(),
                reply_channel: sender,
            })
            .await
            .map_err(|(e, _)| e)?;

        match receiver.await {
            Ok(Ok(())) => {
                // broadcast transaction to peers
                let network_relay = handle.relay::<NetworkService<Waku>>().connect().await?;
                send_transaction(network_relay, tx).await?;
                Ok(res_tx.send(Ok(b"".to_vec().into())).await?)
            }
            Ok(Err(())) => Ok(res_tx
                .send(Err((
                    StatusCode::CONFLICT,
                    "error: unable to add tx".into(),
                )))
                .await?),
            Err(err) => Ok(res_tx
                .send(Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string())))
                .await?),
        }
    } else {
        Err(
            format!("Invalid payload, {payload:?}. Empty or couldn't transform into a utf8 String")
                .into(),
        )
    }
}

async fn handle_waku_info_req(
    waku_channel: &OutboundRelay<NetworkMsg<Waku>>,
    res_tx: &Sender<HttpResponse>,
) -> Result<(), overwatch_rs::DynError> {
    let (sender, receiver) = oneshot::channel();
    waku_channel
        .send(NetworkMsg::Process(WakuBackendMessage::Info {
            reply_channel: sender,
        }))
        .await
        .map_err(|(e, _)| e)?;
    let waku_info: WakuInfo = receiver.await.unwrap();
    res_tx
        .send(Ok(serde_json::to_vec(&waku_info)?.into()))
        .await?;

    Ok(())
}

async fn handle_add_conn_req(
    waku_channel: &OutboundRelay<NetworkMsg<Waku>>,
    res_tx: Sender<HttpResponse>,
    payload: Option<Bytes>,
) -> Result<(), overwatch_rs::DynError> {
    if let Some(payload) = payload {
        if let Ok(addrs) = serde_json::from_slice::<Vec<Multiaddr>>(&payload) {
            let reqs: Vec<_> = addrs
                .into_iter()
                .map(|addr| {
                    waku_channel.send(NetworkMsg::Process(WakuBackendMessage::ConnectPeer {
                        addr,
                    }))
                })
                .collect();

            join_all(reqs).await;
        }
        Ok(res_tx.send(Ok(b"".to_vec().into())).await?)
    } else {
        Err(
            format!("Invalid payload, {payload:?}. Empty or couldn't transform into a utf8 String")
                .into(),
        )
    }
}

async fn send_transaction(
    network_relay: OutboundRelay<NetworkMsg<Waku>>,
    tx: Tx,
) -> Result<(), overwatch_rs::DynError> {
    let payload = wire::serialize(&tx).expect("Tx serialization failed");
    network_relay
        .send(NetworkMsg::Process(WakuBackendMessage::Broadcast {
            message: WakuMessage::new(
                payload,
                WAKU_CARNOT_TX_CONTENT_TOPIC.clone(),
                1,
                chrono::Utc::now().timestamp_nanos() as usize,
                [],
                false,
            ),
            topic: Some(WAKU_CARNOT_PUB_SUB_TOPIC.clone()),
        }))
        .await
        .map_err(|(e, _)| e)?;

    Ok(())
}
