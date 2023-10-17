use std::{net::SocketAddr, sync::Arc};

use axum::{extract::State, response::IntoResponse, routing, Json, Router, Server};
use hyper::StatusCode;
use nomos_core::{da::blob, tx::Transaction};
use nomos_mempool::{openapi::Status, MempoolMetrics};
use overwatch_rs::overwatch::handle::OverwatchHandle;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{Backend, http::{da, cl::{self, Tx}}};

#[derive(Clone)]
pub struct AxumBackendSettings {
    pub da: OverwatchHandle,
    pub addr: SocketAddr,
}

pub struct AxumBackend {
    settings: Arc<AxumBackendSettings>,
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
impl Backend for AxumBackend {
    type Error = hyper::Error;
    type Settings = AxumBackendSettings;

    async fn new(settings: Self::Settings) -> Result<Self, Self::Error>
    where
        Self: Sized,
    {
        Ok(Self {
            settings: Arc::new(settings),
        })
    }

    async fn serve(self) -> Result<(), Self::Error> {
        let store = self.settings.clone();
        let app = Router::new()
            .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
            .route("/da/metrics", routing::get(da_metrics))
            .route("/da/status", routing::post(da_status))
            .route("/cl/metrics", routing::get(cl_metrics))
            .route("/cl/status", routing::post(cl_status))
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
async fn cl_metrics(State(store): State<Store>) -> impl IntoResponse {
    match cl::cl_mempool_metrics(&store.da).await {
        Ok(metrics) => (StatusCode::OK, Json(metrics)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/cl/status",
    responses(
        (status = 200, description = "Query the mempool status of the cl service", body = Vec<<Tx as Transaction>::Hash>),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn cl_status(
    State(store): State<Store>,
    Json(items): Json<Vec<<Tx as Transaction>::Hash>>,
) -> impl IntoResponse {
    match cl::cl_mempool_status(&store.da, items).await {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}