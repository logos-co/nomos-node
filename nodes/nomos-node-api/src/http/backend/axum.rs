use std::{fmt::Debug, hash::Hash, sync::Arc};

use axum::{
    extract::State, http::HeaderValue, response::IntoResponse, routing, Json, Router, Server,
};
use consensus_engine::BlockId;
use full_replication::{Blob, Certificate};
use hyper::{
    header::{CONTENT_TYPE, USER_AGENT},
    StatusCode,
};
use nomos_core::{da::blob, tx::Transaction};
use nomos_mempool::{network::adapters::libp2p::Libp2pAdapter, openapi::Status, MempoolMetrics};
use nomos_network::backends::libp2p::Libp2p;
use nomos_storage::backends::StorageSerde;
use overwatch_rs::overwatch::handle::OverwatchHandle;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    http::{cl, da, info, mempool, storage},
    Backend,
};

/// Configuration for the Http Server
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct AxumBackendSettings {
    /// Socket where the server will be listening on for incoming requests.
    #[cfg_attr(feature = "clap", arg(
        short, long = "http-addr",
        default_value_t = std::net::SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
            8080,
        ),
        env = "HTTP_BIND_ADDRESS"
    ))]
    pub address: std::net::SocketAddr,
    /// Allowed origins for this server deployment requests.
    #[cfg_attr(feature = "clap", arg(long = "http-cors-origin"))]
    pub cors_origins: Vec<String>,
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

// type Store = Arc<AxumBackendSettings>;

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

    async fn serve(self, handle: OverwatchHandle) -> Result<(), Self::Error> {
        let mut builder = CorsLayer::new();
        if self.settings.cors_origins.is_empty() {
            builder = builder.allow_origin(Any);
        }

        for origin in &self.settings.cors_origins {
            builder = builder.allow_origin(
                origin
                    .as_str()
                    .parse::<HeaderValue>()
                    .expect("fail to parse origin"),
            );
        }

        let app = Router::new()
            .layer(
                builder
                    .allow_headers([CONTENT_TYPE, USER_AGENT])
                    .allow_methods(Any),
            )
            .layer(TraceLayer::new_for_http())
            .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
            .route("/da/metrics", routing::get(da_metrics))
            .route("/da/status", routing::post(da_status))
            .route("/da/blob", routing::post(da_blob))
            .route("/cl/metrics", routing::get(cl_metrics::<T>))
            .route("/cl/status", routing::post(cl_status::<T>))
            .route("/carnot/info", routing::get(carnot_info::<T, S, SIZE>))
            .route("/network/info", routing::get(libp2p_info))
            .route("/storage/block", routing::post(block::<S, T>))
            .route("/mempool/add/tx", routing::post(add_tx::<T>))
            .route("/mempool/add/cert", routing::post(add_cert))
            .with_state(handle);

        Server::bind(&self.settings.address)
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
async fn da_metrics(State(handle): State<OverwatchHandle>) -> impl IntoResponse {
    match da::da_mempool_metrics(&handle).await {
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
    State(handle): State<OverwatchHandle>,
    Json(items): Json<Vec<<Blob as blob::Blob>::Hash>>,
) -> impl IntoResponse {
    match da::da_mempool_status(&handle, items).await {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/da/blob",
    responses(
        (status = 200, description = "Query the mempool status of the da service", body = Vec<Blob>),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn da_blob(
    State(handle): State<OverwatchHandle>,
    Json(items): Json<Vec<<Blob as blob::Blob>::Hash>>,
) -> impl IntoResponse {
    match da::da_blob(&handle, items).await {
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
async fn cl_metrics<T>(State(handle): State<OverwatchHandle>) -> impl IntoResponse
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
    match cl::cl_mempool_metrics::<T>(&handle).await {
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
    State(handle): State<OverwatchHandle>,
    Json(items): Json<Vec<<T as Transaction>::Hash>>,
) -> impl IntoResponse
where
    T: Transaction + Clone + Debug + Hash + Serialize + DeserializeOwned + Send + Sync + 'static,
    <T as nomos_core::tx::Transaction>::Hash:
        Serialize + DeserializeOwned + std::cmp::Ord + Debug + Send + Sync + 'static,
{
    match cl::cl_mempool_status::<T>(&handle, items).await {
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
async fn carnot_info<Tx, SS, const SIZE: usize>(
    State(handle): State<OverwatchHandle>,
) -> impl IntoResponse
where
    Tx: Transaction + Clone + Debug + Hash + Serialize + DeserializeOwned + Send + Sync + 'static,
    <Tx as Transaction>::Hash: std::cmp::Ord + Debug + Send + Sync + 'static,
    SS: StorageSerde + Send + Sync + 'static,
{
    match info::carnot_info::<Tx, SS, SIZE>(&handle).await {
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
async fn libp2p_info(State(handle): State<OverwatchHandle>) -> impl IntoResponse {
    match info::libp2p_info(&handle).await {
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
async fn block<S, Tx>(
    State(handle): State<OverwatchHandle>,
    Json(id): Json<BlockId>,
) -> impl IntoResponse
where
    Tx: serde::Serialize + serde::de::DeserializeOwned + Clone + Eq + core::hash::Hash,
    S: StorageSerde + Send + Sync + 'static,
{
    match storage::block_req::<S, Tx>(&handle, id).await {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/mempool/add/tx",
    responses(
        (status = 200, description = "add transaction to the mempool"),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn add_tx<Tx>(State(handle): State<OverwatchHandle>, Json(tx): Json<Tx>) -> impl IntoResponse
where
    Tx: Transaction + Clone + Debug + Hash + Serialize + DeserializeOwned + Send + Sync + 'static,
    <Tx as Transaction>::Hash: std::cmp::Ord + Debug + Send + Sync + 'static,
{
    match mempool::add_tx::<Libp2p, Libp2pAdapter<Tx, <Tx as Transaction>::Hash>, Tx>(&handle, tx)
        .await
    {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/mempool/add/tx",
    responses(
        (status = 200, description = "add certificate to the mempool"),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn add_cert(
    State(handle): State<OverwatchHandle>,
    Json(cert): Json<Certificate>,
) -> impl IntoResponse {
    match mempool::add_cert::<Libp2p, Libp2pAdapter<Certificate, <Blob as blob::Blob>::Hash>>(
        &handle, cert,
    )
    .await
    {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
