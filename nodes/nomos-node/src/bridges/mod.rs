mod libp2p;
use consensus_engine::BlockId;
use libp2p::*;

// std
// crates
use bytes::Bytes;
use http::StatusCode;
use nomos_consensus::{CarnotInfo, ConsensusMsg};
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
use tracing::error;
// internal
use full_replication::{Blob, Certificate};
use nomos_core::wire;
use nomos_core::{
    block::Block,
    da::{blob, certificate::Certificate as _},
    tx::Transaction,
};
use nomos_da::DaMsg;
use nomos_http::backends::axum::AxumBackend;
use nomos_http::bridge::{build_http_bridge, HttpBridgeRunner};
use nomos_http::http::{HttpMethod, HttpRequest, HttpResponse};
use nomos_mempool::backend::mockpool::MockPool;
use nomos_mempool::network::adapters::libp2p::Libp2pAdapter;
use nomos_mempool::network::NetworkAdapter;
use nomos_mempool::{Certificate as CertDiscriminant, Transaction as TxDiscriminant};
use nomos_mempool::{MempoolMetrics, MempoolMsg, MempoolService};
use nomos_network::backends::libp2p::Libp2p;
use nomos_network::backends::NetworkBackend;
use nomos_network::NetworkService;
use nomos_node::{Carnot, Tx};
use nomos_node::{DataAvailability as DataAvailabilityService, Wire};
use nomos_storage::{backends::sled::SledBackend, StorageMsg, StorageService};
use overwatch_rs::services::relay::OutboundRelay;

type DaMempoolService = MempoolService<
    Libp2pAdapter<Certificate, <Blob as blob::Blob>::Hash>,
    MockPool<Certificate, <Blob as blob::Blob>::Hash>,
    CertDiscriminant,
>;
type ClMempoolService = MempoolService<
    Libp2pAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<Tx, <Tx as Transaction>::Hash>,
    TxDiscriminant,
>;

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

macro_rules! post_handler {
    ($handle:expr, $service:ty, $path:expr => $handler:tt) => {{
        let (channel, mut http_request_channel) =
            build_http_bridge::<$service, AxumBackend, _>($handle, HttpMethod::POST, $path)
                .await
                .unwrap();
        while let Some(HttpRequest {
            res_tx, payload, ..
        }) = http_request_channel.recv().await
        {
            if let Err(e) = $handler(&channel, payload, res_tx).await {
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

pub fn cl_mempool_metrics_bridge(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner {
    Box::new(Box::pin(async move {
        get_handler!(handle, ClMempoolService, "metrics" => handle_mempool_metrics_req)
    }))
}

pub fn da_mempool_metrics_bridge(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner {
    Box::new(Box::pin(async move {
        get_handler!(handle, DaMempoolService, "metrics" => handle_mempool_metrics_req)
    }))
}

pub fn da_mempool_status_bridge(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner {
    Box::new(Box::pin(async move {
        post_handler!(handle, DaMempoolService, "status" => handle_mempool_status_req)
    }))
}

pub fn cl_mempool_status_bridge(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner {
    Box::new(Box::pin(async move {
        post_handler!(handle, ClMempoolService, "status" => handle_mempool_status_req)
    }))
}

pub fn storage_get_blocks_bridge(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner {
    Box::new(Box::pin(async move {
        post_handler!(handle, StorageService<SledBackend<Wire>>, "block" => handle_block_get_req)
    }))
}

pub fn network_info_bridge(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner {
    Box::new(Box::pin(async move {
        get_handler!(handle, NetworkService<Libp2p>, "info" => handle_libp2p_info_req)
    }))
}

pub fn da_blob_get_bridge(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner {
    Box::new(Box::pin(async move {
        post_handler!(handle, DataAvailabilityService, "blobs" => handle_da_blobs_req)
    }))
}

pub async fn handle_da_blobs_req<B>(
    da_channel: &OutboundRelay<DaMsg<B>>,
    payload: Option<Bytes>,
    res_tx: Sender<HttpResponse>,
) -> Result<(), overwatch_rs::DynError>
where
    B: blob::Blob + Serialize,
    B::Hash: DeserializeOwned + Send + 'static,
{
    let (reply_channel, receiver) = oneshot::channel();
    let ids: Vec<B::Hash> = serde_json::from_slice(payload.unwrap_or_default().as_ref())?;
    da_channel
        .send(DaMsg::Get {
            ids: Box::new(ids.into_iter()),
            reply_channel,
        })
        .await
        .map_err(|(e, _)| e)?;

    let blobs = receiver.await.unwrap();
    res_tx
        .send(Ok(serde_json::to_string(&blobs).unwrap().into()))
        .await?;

    Ok(())
}

pub async fn handle_block_get_req(
    storage_channel: &OutboundRelay<StorageMsg<SledBackend<Wire>>>,
    payload: Option<Bytes>,
    res_tx: Sender<HttpResponse>,
) -> Result<(), overwatch_rs::DynError> {
    let key: BlockId = serde_json::from_slice(payload.unwrap_or_default().as_ref())?;
    let (msg, receiver) = StorageMsg::new_load_message(key);
    storage_channel.send(msg).await.map_err(|(e, _)| e)?;
    let block: Option<Block<Tx, Certificate>> = receiver.recv().await?;
    res_tx
        .send(Ok(serde_json::to_string(&block).unwrap().into()))
        .await?;

    Ok(())
}

pub async fn handle_mempool_status_req<K, V>(
    mempool_channel: &OutboundRelay<MempoolMsg<V, K>>,
    payload: Option<Bytes>,
    res_tx: Sender<HttpResponse>,
) -> Result<(), overwatch_rs::DynError>
where
    K: DeserializeOwned,
{
    let (sender, receiver) = oneshot::channel();
    let items: Vec<K> = serde_json::from_slice(payload.unwrap_or_default().as_ref())?;
    mempool_channel
        .send(MempoolMsg::Status {
            items,
            reply_channel: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    let status = receiver.await.unwrap();
    res_tx
        .send(Ok(serde_json::to_string(&status).unwrap().into()))
        .await?;

    Ok(())
}

pub fn mempool_add_tx_bridge<N, A>(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner
where
    N: NetworkBackend,
    A: NetworkAdapter<Backend = N, Item = Tx, Key = <Tx as Transaction>::Hash>
        + Send
        + Sync
        + 'static,
    A::Settings: Send + Sync,
{
    Box::new(Box::pin(async move {
        let (mempool_channel, mut http_request_channel) =
            build_http_bridge::<
                MempoolService<A, MockPool<Tx, <Tx as Transaction>::Hash>, TxDiscriminant>,
                AxumBackend,
                _,
            >(handle.clone(), HttpMethod::POST, "add")
            .await
            .unwrap();

        while let Some(HttpRequest {
            res_tx, payload, ..
        }) = http_request_channel.recv().await
        {
            if let Err(e) = handle_mempool_add_req(
                &mempool_channel,
                res_tx,
                payload.unwrap_or_default(),
                |tx| tx.hash(),
            )
            .await
            {
                error!(e);
            }
        }
        Ok(())
    }))
}

pub fn mempool_add_cert_bridge<N, A>(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner
where
    N: NetworkBackend,
    A: NetworkAdapter<Backend = N, Item = Certificate, Key = <Blob as blob::Blob>::Hash>
        + Send
        + Sync
        + 'static,
    A::Settings: Send + Sync,
{
    Box::new(Box::pin(async move {
        let (mempool_channel, mut http_request_channel) = build_http_bridge::<
            MempoolService<A, MockPool<Certificate, <Blob as blob::Blob>::Hash>, CertDiscriminant>,
            AxumBackend,
            _,
        >(
            handle.clone(), HttpMethod::POST, "add"
        )
        .await
        .unwrap();

        while let Some(HttpRequest {
            res_tx, payload, ..
        }) = http_request_channel.recv().await
        {
            if let Err(e) = handle_mempool_add_req(
                &mempool_channel,
                res_tx,
                payload.unwrap_or_default(),
                |cert| cert.hash(),
            )
            .await
            {
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

async fn handle_mempool_metrics_req<K, V>(
    mempool_channel: &OutboundRelay<MempoolMsg<K, V>>,
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
            "{{\"pending_items\": {}, \"last_item\": {}}}",
            metrics.pending_items, metrics.last_item_timestamp
        )
        .into()))
        .await?;

    Ok(())
}

pub(super) async fn handle_mempool_add_req<K, V>(
    mempool_channel: &OutboundRelay<MempoolMsg<K, V>>,
    res_tx: Sender<HttpResponse>,
    wire_item: Bytes,
    key: impl Fn(&K) -> V,
) -> Result<(), overwatch_rs::DynError>
where
    K: DeserializeOwned,
{
    let item = wire::deserialize::<K>(&wire_item)?;
    let (sender, receiver) = oneshot::channel();
    let key = key(&item);
    mempool_channel
        .send(MempoolMsg::Add {
            item,
            key,
            reply_channel: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    match receiver.await {
        Ok(Ok(())) => Ok(res_tx.send(Ok(b"".to_vec().into())).await?),
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
}
