// std

// crates
use tokio::sync::oneshot;

// internal
use crate::tx::{Tx, TxId};
use nomos_http::backends::axum::AxumBackend;
use nomos_http::bridge::{build_http_bridge, HttpBridgeRunner};
use nomos_http::http::{HttpMethod, HttpRequest};
use nomos_mempool::backend::mockpool::MockPool;
use nomos_mempool::network::adapters::waku::WakuAdapter;
use nomos_mempool::{MempoolMetrics, MempoolMsg, MempoolService};

pub fn mempool_metrics_bridge(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner {
    Box::new(Box::pin(async move {
        let (mempool_channel, mut http_request_channel) = build_http_bridge::<
            MempoolService<WakuAdapter<Tx>, MockPool<TxId, Tx>>,
            AxumBackend,
            _,
        >(
            handle,
            HttpMethod::GET,
            "mempool/metrics",
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
                .send(format!("{{\"pending_tx\": {}}}", metrics.pending_txs).into())
                .await
                .unwrap();
        }
        Ok(())
    }))
}
