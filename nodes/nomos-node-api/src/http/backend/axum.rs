use std::{fmt::Debug, hash::Hash, net::SocketAddr, sync::Arc};

use axum::{extract::State, response::Response, routing, Json, Router, Server};
use full_replication::Blob;
use nomos_core::{da::blob, tx::Transaction};
use nomos_mempool::{openapi::Status, MempoolMetrics};
use nomos_storage::backends::StorageSerde;
use overwatch_rs::overwatch::handle::OverwatchHandle;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    http::{cl, consensus, da, libp2p},
    Backend,
};

#[derive(Clone)]
pub struct AxumBackendSettings {
    pub addr: SocketAddr,
    pub handle: OverwatchHandle,
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
            .with_state(store);

        Server::bind(&self.settings.addr)
            .serve(app.into_make_service())
            .await
    }
}

macro_rules! make_request_and_return_response {
    ($mod:tt::$fn:tt $(:: < $($generic: ident), + $(,)? > )? ($handle:ident $(,)? $($($param:ident),+ $(,)?)? )) => {{
        match $mod::$fn $(:: <$($generic),+> )? (&$handle.handle, $($($param),+)?).await {
            ::std::result::Result::Ok(val) => ::axum::response::IntoResponse::into_response((::hyper::StatusCode::OK, ::axum::Json(val))),
            ::std::result::Result::Err(e) => ::axum::response::IntoResponse::into_response((::hyper::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
        }
    }};
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
    make_request_and_return_response!(da::da_mempool_metrics(store))
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
    make_request_and_return_response!(da::da_mempool_status(store, items))
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
    make_request_and_return_response!(da::da_blobs(store, items))
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
    make_request_and_return_response!(cl::cl_mempool_metrics::<T>(store))
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
    make_request_and_return_response!(cl::cl_mempool_status::<T>(store, items))
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
    make_request_and_return_response!(consensus::carnot_info::<Tx, SS, SIZE>(store))
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
    make_request_and_return_response!(libp2p::libp2p_info(store))
}
