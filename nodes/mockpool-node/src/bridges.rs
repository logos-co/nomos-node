// std
// crates
use tokio::sync::oneshot;
use tracing::debug;
// internal
use crate::tx::{Tx, TxId};
use futures::future::join_all;
use multiaddr::Multiaddr;
use nomos_core::wire;
use nomos_http::backends::axum::AxumBackend;
use nomos_http::bridge::{build_http_bridge, HttpBridgeRunner};
use nomos_http::http::{HttpMethod, HttpRequest};
use nomos_mempool::backend::mockpool::MockPool;
use nomos_mempool::network::adapters::waku::{
    WakuAdapter, WAKU_CARNOT_PUB_SUB_TOPIC, WAKU_CARNOT_TX_CONTENT_TOPIC,
};
use nomos_mempool::{MempoolMetrics, MempoolMsg, MempoolService};
use nomos_network::backends::waku::{Waku, WakuBackendMessage, WakuInfo};
use nomos_network::{NetworkMsg, NetworkService};
use overwatch_rs::services::relay::OutboundRelay;
use waku_bindings::WakuMessage;

pub fn mempool_metrics_bridge(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner {
    Box::new(Box::pin(async move {
        let (mempool_channel, mut http_request_channel) = build_http_bridge::<
            MempoolService<WakuAdapter<Tx>, MockPool<TxId, Tx>>,
            AxumBackend,
            _,
        >(
            handle, HttpMethod::GET, "metrics"
        )
        .await
        .unwrap();

        while let Some(HttpRequest { res_tx, .. }) = http_request_channel.recv().await {
            let (sender, receiver) = oneshot::channel();
            mempool_channel
                .send(MempoolMsg::Metrics {
                    reply_channel: sender,
                })
                .await
                .unwrap();
            let metrics: MempoolMetrics = receiver.await.unwrap();
            res_tx
                // TODO: use serde to serialize metrics
                .send(format!("{{\"pending_tx\": {}}}", metrics.pending_txs).into())
                .await
                .unwrap();
        }
        Ok(())
    }))
}

pub fn mempool_add_tx_bridge(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner {
    Box::new(Box::pin(async move {
        let (mempool_channel, mut http_request_channel) = build_http_bridge::<
            MempoolService<WakuAdapter<Tx>, MockPool<TxId, Tx>>,
            AxumBackend,
            _,
        >(
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
                    .unwrap();
                if let Ok(()) = receiver.await.unwrap() {
                    // broadcast transaction to peers
                    let network_relay = handle
                        .relay::<NetworkService<Waku>>()
                        .connect()
                        .await
                        .unwrap();
                    send_transaction(network_relay, tx).await;
                    res_tx.send(b"".to_vec().into()).await.unwrap();
                }
            } else {
                debug!(
                    "Invalid payload, {:?}. Empty or couldn't transform into a utf8 String",
                    payload
                );
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
            let (sender, receiver) = oneshot::channel();
            waku_channel
                .send(NetworkMsg::Process(WakuBackendMessage::Info {
                    reply_channel: sender,
                }))
                .await
                .unwrap();
            let waku_info: WakuInfo = receiver.await.unwrap();
            res_tx
                .send(
                    serde_json::to_vec(&waku_info)
                        .expect("Serializing of waku info message should not fail")
                        .into(),
                )
                .await
                .unwrap();
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
            if let Some(payload) = payload {
                if let Ok(addrs) = serde_json::from_slice::<Vec<Multiaddr>>(&payload) {
                    let reqs: Vec<_> = addrs
                        .into_iter()
                        .map(|addr| {
                            waku_channel.send(NetworkMsg::Process(
                                WakuBackendMessage::ConnectPeer { addr },
                            ))
                        })
                        .collect();

                    join_all(reqs).await;
                }
                res_tx.send(b"".to_vec().into()).await.unwrap();
            } else {
                debug!("Invalid payload, {:?}. Should not be empty", payload);
            }
        }
        Ok(())
    }))
}

async fn send_transaction(network_relay: OutboundRelay<NetworkMsg<Waku>>, tx: Tx) {
    let payload = wire::serialize(&tx).expect("Tx serialization failed");
    if let Err((_, _e)) = network_relay
        .send(NetworkMsg::Process(WakuBackendMessage::Broadcast {
            message: WakuMessage::new(
                payload,
                WAKU_CARNOT_TX_CONTENT_TOPIC.clone(),
                1,
                chrono::Utc::now().timestamp() as usize,
            ),
            topic: Some(WAKU_CARNOT_PUB_SUB_TOPIC.clone()),
        }))
        .await
    {
        todo!("log error");
    };
}
