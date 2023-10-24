use std::{fmt::Debug, hash::Hash, net::SocketAddr, sync::Arc};

use axum::{extract::State, response::IntoResponse, routing, Json, Router, Server};
use full_replication::Blob;
use hyper::StatusCode;
use nomos_core::{da::blob, tx::Transaction};
use nomos_mempool::{openapi::Status, MempoolMetrics};
use overwatch_rs::overwatch::handle::OverwatchHandle;
use serde::{Deserialize, Serialize};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    http::{cl, da},
    Backend,
};

#[derive(Clone)]
pub struct AxumBackendSettings {
    pub addr: SocketAddr,
    pub da: OverwatchHandle,
    pub cl: OverwatchHandle,
}

pub struct AxumBackend<ClTransaction> {
    settings: Arc<AxumBackendSettings>,
    _cl: core::marker::PhantomData<ClTransaction>,
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
impl<ClTransaction> Backend for AxumBackend<ClTransaction>
where
    ClTransaction: Transaction
        + Clone
        + Debug
        + Hash
        + Serialize
        + for<'de> Deserialize<'de>
        + Send
        + Sync
        + 'static,
    <ClTransaction as nomos_core::tx::Transaction>::Hash:
        Serialize + for<'de> Deserialize<'de> + std::cmp::Ord + Debug + Send + Sync + 'static,
{
    type Error = hyper::Error;
    type Settings = AxumBackendSettings;

    async fn new(settings: Self::Settings) -> Result<Self, Self::Error>
    where
        Self: Sized,
    {
        Ok(Self {
            settings: Arc::new(settings),
            _cl: core::marker::PhantomData,
        })
    }

    async fn serve(self) -> Result<(), Self::Error> {
        let store = self.settings.clone();
        let app = Router::new()
            .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
            .route("/da/metrics", routing::get(da_metrics))
            .route("/da/status", routing::post(da_status))
            .route("/cl/metrics", routing::get(cl_metrics::<ClTransaction>))
            .route("/cl/status", routing::post(cl_status::<ClTransaction>))
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
async fn da_metrics(State(store): State<Store>) -> impl IntoResponse {
    match da::da_mempool_metrics(&store.da).await {
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
) -> impl IntoResponse {
    match da::da_mempool_status(&store.da, items).await {
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
async fn cl_metrics<T>(State(store): State<Store>) -> impl IntoResponse
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
) -> impl IntoResponse
where
    T: Transaction
        + Clone
        + Debug
        + Hash
        + Serialize
        + serde::de::DeserializeOwned
        + Send
        + Sync
        + 'static,
    <T as nomos_core::tx::Transaction>::Hash:
        Serialize + serde::de::DeserializeOwned + std::cmp::Ord + Debug + Send + Sync + 'static,
{
    match cl::cl_mempool_status::<T>(&store.cl, items).await {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
