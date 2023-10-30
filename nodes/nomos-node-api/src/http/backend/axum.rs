use std::{fmt::Debug, hash::Hash, net::SocketAddr, sync::Arc};

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    routing, Json, Router, Server,
};
use consensus_engine::BlockId;
use full_replication::Blob;
use hyper::StatusCode;
use nomos_core::{da::blob, tx::Transaction};
use nomos_mempool::{openapi::Status, MempoolMetrics};
use nomos_storage::backends::StorageSerde;
use overwatch_rs::overwatch::handle::OverwatchHandle;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    http::{cl, da, info, storage},
    Backend,
};

#[derive(Clone)]
pub struct AxumBackendSettings {
    pub addr: SocketAddr,
    pub da_mempool: OverwatchHandle,
    pub da: OverwatchHandle,
    pub cl: OverwatchHandle,
    pub carnot: OverwatchHandle,
    pub network: OverwatchHandle,
    pub storage: OverwatchHandle,
}

pub struct AxumBackend<T, S, const SIZE: usize> {
    settings: Arc<AxumBackendSettings>,
    _tx: core::marker::PhantomData<T>,
    _storage_serde: core::marker::PhantomData<S>,
}

#[derive(OpenApi)]
#[openapi(
    paths(
        da_metrics,
        da_status,
    ),
    components(
        schemas(Status, MempoolMetrics)
    ),
    tags(
        (name = "da", description = "data availibility related APIs")
    )
)]
struct ApiDoc;

type Store = Arc<AxumBackendSettings>;

#[async_trait::async_trait]
impl<T, S, const SIZE: usize> Backend for AxumBackend<T, S, SIZE>
where
    T: Transaction
        + Clone
        + Debug
        + Eq
        + Hash
        + Serialize
        + for<'de> Deserialize<'de>
        + Send
        + Sync
        + 'static,
    <T as nomos_core::tx::Transaction>::Hash:
        Serialize + for<'de> Deserialize<'de> + std::cmp::Ord + Debug + Send + Sync + 'static,
    S: StorageSerde + Send + Sync + 'static,
{
    type Error = hyper::Error;
    type Settings = AxumBackendSettings;

    async fn new(settings: Self::Settings) -> Result<Self, Self::Error>
    where
        Self: Sized,
    {
        Ok(Self {
            settings: Arc::new(settings),
            _tx: core::marker::PhantomData,
            _storage_serde: core::marker::PhantomData,
        })
    }

    async fn serve(self) -> Result<(), Self::Error> {
        let store = self.settings.clone();
        let app = Router::new()
            .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
            .route("/da/metrics", routing::get(da_metrics))
            .route("/da/status", routing::post(da_status))
            .route("/da/blobs", routing::post(da_blobs))
            .route("/cl/metrics", routing::get(cl_metrics::<T>))
            .route("/cl/status", routing::post(cl_status::<T>))
            .route("/carnot/info", routing::get(carnot_info::<T, S, SIZE>))
            .route("/network/info", routing::get(libp2p_info))
            .route("/storage/block", routing::post(block::<S, T>))
            .with_state(store);

        Server::bind(&self.settings.addr)
            .serve(app.into_make_service())
            .await
    }
}

#[utoipa::path(
    get,
    path = "/da/metrics",
    responses(
        (status = 200, description = "Get the mempool metrics of the da service", body = MempoolMetrics),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn da_metrics(State(store): State<Store>) -> Response {
    match da::da_mempool_metrics(&store.da_mempool).await {
        Ok(metrics) => (StatusCode::OK, Json(metrics)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/da/status",
    responses(
        (status = 200, description = "Query the mempool status of the da service", body = Vec<<Blob as blob::Blob>::Hash>),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn da_status(
    State(store): State<Store>,
    Json(items): Json<Vec<<Blob as blob::Blob>::Hash>>,
) -> Response {
    match da::da_mempool_status(&store.da_mempool, items).await {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/da/blobs",
    responses(
        (status = 200, description = "Get pending blobs", body = Vec<Blob>),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn da_blobs(
    State(store): State<Store>,
    Json(items): Json<Vec<<Blob as blob::Blob>::Hash>>,
) -> Response {
    match da::da_blob(&store.da, items).await {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/cl/metrics",
    responses(
        (status = 200, description = "Get the mempool metrics of the cl service", body = MempoolMetrics),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn cl_metrics<T>(State(store): State<Store>) -> Response
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
    match cl::cl_mempool_metrics::<T>(&store.cl).await {
        Ok(metrics) => (StatusCode::OK, Json(metrics)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/cl/status",
    responses(
        (status = 200, description = "Query the mempool status of the cl service", body = Vec<<T as Transaction>::Hash>),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn cl_status<T>(
    State(store): State<Store>,
    Json(items): Json<Vec<<T as Transaction>::Hash>>,
) -> Response
where
    T: Transaction + Clone + Debug + Hash + Serialize + DeserializeOwned + Send + Sync + 'static,
    <T as nomos_core::tx::Transaction>::Hash:
        Serialize + DeserializeOwned + std::cmp::Ord + Debug + Send + Sync + 'static,
{
    match cl::cl_mempool_status::<T>(&store.cl, items).await {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/carnot/info",
    responses(
        (status = 200, description = "Query the carnot information", body = nomos_consensus::CarnotInfo),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn carnot_info<Tx, SS, const SIZE: usize>(State(store): State<Store>) -> Response
where
    Tx: Transaction + Clone + Debug + Hash + Serialize + DeserializeOwned + Send + Sync + 'static,
    <Tx as Transaction>::Hash: std::cmp::Ord + Debug + Send + Sync + 'static,
    SS: StorageSerde + Send + Sync + 'static,
{
    match info::carnot_info::<Tx, SS, SIZE>(&store.carnot).await {
        Ok(info) => (StatusCode::OK, Json(info)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/network/info",
    responses(
        (status = 200, description = "Query the network information", body = nomos_network::backends::libp2p::Libp2pInfo),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn libp2p_info(State(store): State<Store>) -> Response {
    match info::libp2p_info(&store.network).await {
        Ok(info) => (StatusCode::OK, Json(info)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/storage/block",
    responses(
        (status = 200, description = "Get the block by block id", body = Block<Tx, full_replication::Certificate>),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn block<S, Tx>(State(store): State<Store>, Json(id): Json<BlockId>) -> Response
where
    Tx: serde::Serialize + serde::de::DeserializeOwned + Clone + Eq + core::hash::Hash,
    S: StorageSerde + Send + Sync + 'static,
{
    match storage::block_req::<S, Tx>(&store.storage, id).await {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
