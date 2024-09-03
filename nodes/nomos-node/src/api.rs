// std
use std::error::Error;
use std::ops::Range;
use std::{fmt::Debug, hash::Hash};
// crates
use axum::{
    extract::{Query, State},
    http::HeaderValue,
    response::{IntoResponse, Response},
    routing, Json, Router, Server,
};
use hyper::{
    header::{CONTENT_TYPE, USER_AGENT},
    Body, StatusCode,
};
use nomos_api::{
    http::{cl, consensus, da, libp2p, mempool, metrics, storage},
    Backend,
};
use nomos_core::da::blob::info::DispersedBlobInfo;
use nomos_core::da::blob::metadata::Metadata;
use nomos_core::da::DaVerifier as CoreDaVerifier;
use nomos_core::{da::blob::Blob, header::HeaderId, tx::Transaction};
use nomos_da_network_core::SubnetworkId;
use nomos_da_sampling::backend::DaSamplingServiceBackend;
use nomos_da_verifier::backend::VerifierBackend;
use nomos_libp2p::PeerId;
use nomos_mempool::{
    network::adapters::libp2p::Libp2pAdapter as MempoolNetworkAdapter,
    tx::service::openapi::Status, MempoolMetrics,
};
use nomos_network::backends::libp2p::Libp2p as NetworkBackend;
use nomos_storage::backends::StorageSerde;
use overwatch_rs::overwatch::handle::OverwatchHandle;
use rand::{RngCore, SeedableRng};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use subnetworks_assignations::MembershipHandler;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
// internal

/// Configuration for the Http Server
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct AxumBackendSettings {
    /// Socket where the server will be listening on for incoming requests.
    pub address: std::net::SocketAddr,
    /// Allowed origins for this server deployment requests.
    pub cors_origins: Vec<String>,
}

pub struct AxumBackend<
    A,
    B,
    C,
    M,
    V,
    VB,
    T,
    S,
    SamplingBackend,
    SamplingNetworkAdapter,
    SamplingRng,
    const SIZE: usize,
> {
    settings: AxumBackendSettings,
    _attestation: core::marker::PhantomData<A>,
    _blob: core::marker::PhantomData<B>,
    _certificate: core::marker::PhantomData<C>,
    _membership: core::marker::PhantomData<M>,
    _vid: core::marker::PhantomData<V>,
    _verifier_backend: core::marker::PhantomData<VB>,
    _tx: core::marker::PhantomData<T>,
    _storage_serde: core::marker::PhantomData<S>,
    _sampling_backend: core::marker::PhantomData<SamplingBackend>,
    _sampling_network_adapter: core::marker::PhantomData<SamplingNetworkAdapter>,
    _sampling_rng: core::marker::PhantomData<SamplingRng>,
}

#[derive(OpenApi)]
#[openapi(
    paths(
    ),
    components(
        schemas(Status<HeaderId>, MempoolMetrics)
    ),
    tags(
        (name = "da", description = "data availibility related APIs")
    )
)]
struct ApiDoc;

#[async_trait::async_trait]
impl<
        A,
        B,
        C,
        M,
        V,
        VB,
        T,
        S,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        const SIZE: usize,
    > Backend
    for AxumBackend<
        A,
        B,
        C,
        M,
        V,
        VB,
        T,
        S,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SIZE,
    >
where
    A: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    B: Blob + Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    <B as Blob>::BlobId: AsRef<[u8]> + Send + Sync + 'static,
    <B as Blob>::ColumnIndex: AsRef<[u8]> + Send + Sync + 'static,
    C: DispersedBlobInfo<BlobId = [u8; 32]>
        + Clone
        + Debug
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    <C as DispersedBlobInfo>::BlobId: Clone + Send + Sync,
    M: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
    V: DispersedBlobInfo<BlobId = [u8; 32]>
        + From<C>
        + Eq
        + Debug
        + Metadata
        + Hash
        + Clone
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    <V as DispersedBlobInfo>::BlobId: Debug + Clone + Ord + Hash,
    <V as Metadata>::AppId: AsRef<[u8]> + Clone + Serialize + DeserializeOwned + Send + Sync,
    <V as Metadata>::Index:
        AsRef<[u8]> + Clone + Serialize + DeserializeOwned + PartialOrd + Send + Sync,
    VB: VerifierBackend + CoreDaVerifier<DaBlob = B> + Send + Sync + 'static,
    <VB as VerifierBackend>::Settings: Clone,
    <VB as CoreDaVerifier>::Error: Error,
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
    SamplingRng: SeedableRng + RngCore + Send + 'static,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng> + Send + 'static,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter + Send + 'static,
{
    type Error = hyper::Error;
    type Settings = AxumBackendSettings;

    async fn new(settings: Self::Settings) -> Result<Self, Self::Error>
    where
        Self: Sized,
    {
        Ok(Self {
            settings,
            _attestation: core::marker::PhantomData,
            _blob: core::marker::PhantomData,
            _certificate: core::marker::PhantomData,
            _membership: core::marker::PhantomData,
            _vid: core::marker::PhantomData,
            _verifier_backend: core::marker::PhantomData,
            _tx: core::marker::PhantomData,
            _storage_serde: core::marker::PhantomData,
            _sampling_backend: core::marker::PhantomData,
            _sampling_network_adapter: core::marker::PhantomData,
            _sampling_rng: core::marker::PhantomData,
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
            .route("/cl/metrics", routing::get(cl_metrics::<T>))
            .route("/cl/status", routing::post(cl_status::<T>))
            .route(
                "/cryptarchia/info",
                routing::get(
                    cryptarchia_info::<
                        T,
                        S,
                        SamplingBackend,
                        SamplingNetworkAdapter,
                        SamplingRng,
                        SIZE,
                    >,
                ),
            )
            .route(
                "/cryptarchia/headers",
                routing::get(
                    cryptarchia_headers::<
                        T,
                        S,
                        SamplingBackend,
                        SamplingNetworkAdapter,
                        SamplingRng,
                        SIZE,
                    >,
                ),
            )
            .route("/da/add_blob", routing::post(add_blob::<A, B, M, VB, S>))
            .route(
                "/da/get_range",
                routing::post(
                    get_range::<
                        T,
                        C,
                        V,
                        S,
                        SamplingBackend,
                        SamplingNetworkAdapter,
                        SamplingRng,
                        SIZE,
                    >,
                ),
            )
            .route("/network/info", routing::get(libp2p_info))
            .route("/storage/block", routing::post(block::<S, T>))
            .route("/mempool/add/tx", routing::post(add_tx::<T>))
            .route("/mempool/add/blobinfo", routing::post(add_blob_info::<V>))
            .route("/metrics", routing::get(get_metrics))
            .with_state(handle);

        Server::bind(&self.settings.address)
            .serve(app.into_make_service())
            .await
    }
}

macro_rules! make_request_and_return_response {
    ($cond:expr) => {{
        match $cond.await {
            ::std::result::Result::Ok(val) => ::axum::response::IntoResponse::into_response((
                ::hyper::StatusCode::OK,
                ::axum::Json(val),
            )),
            ::std::result::Result::Err(e) => ::axum::response::IntoResponse::into_response((
                ::hyper::StatusCode::INTERNAL_SERVER_ERROR,
                e.to_string(),
            )),
        }
    }};
}

#[utoipa::path(
    get,
    path = "/cl/metrics",
    responses(
        (status = 200, description = "Get the mempool metrics of the cl service", body = MempoolMetrics),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn cl_metrics<T>(State(handle): State<OverwatchHandle>) -> Response
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
    make_request_and_return_response!(cl::cl_mempool_metrics::<T>(&handle))
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
) -> Response
where
    T: Transaction + Clone + Debug + Hash + Serialize + DeserializeOwned + Send + Sync + 'static,
    <T as nomos_core::tx::Transaction>::Hash:
        Serialize + DeserializeOwned + std::cmp::Ord + Debug + Send + Sync + 'static,
{
    make_request_and_return_response!(cl::cl_mempool_status::<T>(&handle, items))
}
#[derive(Deserialize)]
#[allow(dead_code)]
struct QueryParams {
    from: Option<HeaderId>,
    to: Option<HeaderId>,
}

#[utoipa::path(
    get,
    path = "/cryptarchia/info",
    responses(
        (status = 200, description = "Query consensus information", body = nomos_consensus::CryptarchiaInfo),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn cryptarchia_info<
    Tx,
    SS,
    SamplingBackend,
    SamplingNetworkAdapter,
    SamplingRng,
    const SIZE: usize,
>(
    State(handle): State<OverwatchHandle>,
) -> Response
where
    Tx: Transaction
        + Clone
        + Eq
        + Debug
        + Hash
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    <Tx as Transaction>::Hash: std::cmp::Ord + Debug + Send + Sync + 'static,
    SS: StorageSerde + Send + Sync + 'static,
    SamplingRng: SeedableRng + RngCore,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter,
{
    make_request_and_return_response!(consensus::cryptarchia_info::<
        Tx,
        SS,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SIZE,
    >(&handle))
}

#[utoipa::path(
    get,
    path = "/cryptarchia/headers",
    responses(
        (status = 200, description = "Query header ids", body = Vec<HeaderId>),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn cryptarchia_headers<
    Tx,
    SS,
    SamplingBackend,
    SamplingNetworkAdapter,
    SamplingRng,
    const SIZE: usize,
>(
    State(store): State<OverwatchHandle>,
    Query(query): Query<QueryParams>,
) -> Response
where
    Tx: Transaction
        + Eq
        + Clone
        + Debug
        + Hash
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    <Tx as Transaction>::Hash: std::cmp::Ord + Debug + Send + Sync + 'static,
    SS: StorageSerde + Send + Sync + 'static,
    SamplingRng: SeedableRng + RngCore,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter,
{
    let QueryParams { from, to } = query;
    make_request_and_return_response!(consensus::cryptarchia_headers::<
        Tx,
        SS,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SIZE,
    >(&store, from, to))
}

#[utoipa::path(
    post,
    path = "/da/add_blob",
    responses(
        (status = 200, description = "Attestation for DA blob", body = Option<Attestation>),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn add_blob<A, B, M, VB, SS>(
    State(handle): State<OverwatchHandle>,
    Json(blob): Json<B>,
) -> Response
where
    A: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    B: Blob + Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    <B as Blob>::BlobId: AsRef<[u8]> + Send + Sync + 'static,
    <B as Blob>::ColumnIndex: AsRef<[u8]> + Send + Sync + 'static,
    M: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
    VB: VerifierBackend + CoreDaVerifier<DaBlob = B>,
    <VB as VerifierBackend>::Settings: Clone,
    <VB as CoreDaVerifier>::Error: Error,
    SS: StorageSerde + Send + Sync + 'static,
{
    make_request_and_return_response!(da::add_blob::<A, B, M, VB, SS>(&handle, blob))
}

#[derive(Serialize, Deserialize)]
struct GetRangeReq<V: Metadata>
where
    <V as Metadata>::AppId: Serialize + DeserializeOwned,
    <V as Metadata>::Index: Serialize + DeserializeOwned,
{
    app_id: <V as Metadata>::AppId,
    range: Range<<V as Metadata>::Index>,
}

#[utoipa::path(
    post,
    path = "/da/get_range",
    responses(
        (status = 200, description = "Range of blobs", body = Vec<([u8;8], Option<DaBlob>)>),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn get_range<
    Tx,
    C,
    V,
    SS,
    SamplingBackend,
    SamplingNetworkAdapter,
    SamplingRng,
    const SIZE: usize,
>(
    State(handle): State<OverwatchHandle>,
    Json(GetRangeReq { app_id, range }): Json<GetRangeReq<V>>,
) -> Response
where
    Tx: Transaction
        + Eq
        + Clone
        + Debug
        + Hash
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    <Tx as Transaction>::Hash: std::cmp::Ord + Debug + Send + Sync + 'static,
    C: DispersedBlobInfo<BlobId = [u8; 32]>
        + Clone
        + Debug
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    <C as DispersedBlobInfo>::BlobId: Clone + Send + Sync,
    V: DispersedBlobInfo<BlobId = [u8; 32]>
        + From<C>
        + Eq
        + Debug
        + Metadata
        + Hash
        + Clone
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    <V as DispersedBlobInfo>::BlobId: Debug + Clone + Ord + Hash,
    <V as Metadata>::AppId: AsRef<[u8]> + Clone + Serialize + DeserializeOwned + Send + Sync,
    <V as Metadata>::Index:
        AsRef<[u8]> + Clone + Serialize + DeserializeOwned + PartialOrd + Send + Sync,
    SS: StorageSerde + Send + Sync + 'static,
    SamplingRng: SeedableRng + RngCore,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter,
{
    make_request_and_return_response!(da::get_range::<
        Tx,
        C,
        V,
        SS,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SIZE,
    >(&handle, app_id, range))
}

#[utoipa::path(
    get,
    path = "/network/info",
    responses(
        (status = 200, description = "Query the network information", body = nomos_network::backends::libp2p::Libp2pInfo),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn libp2p_info(State(handle): State<OverwatchHandle>) -> Response {
    make_request_and_return_response!(libp2p::libp2p_info(&handle))
}

#[utoipa::path(
    get,
    path = "/storage/block",
    responses(
        (status = 200, description = "Get the block by block id", body = Block<Tx, kzgrs_backend::dispersal::BlobInfo>),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn block<S, Tx>(State(handle): State<OverwatchHandle>, Json(id): Json<HeaderId>) -> Response
where
    Tx: serde::Serialize + serde::de::DeserializeOwned + Clone + Eq + core::hash::Hash,
    S: StorageSerde + Send + Sync + 'static,
{
    make_request_and_return_response!(storage::block_req::<S, Tx>(&handle, id))
}

#[utoipa::path(
    post,
    path = "/mempool/add/tx",
    responses(
        (status = 200, description = "Add transaction to the mempool"),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn add_tx<Tx>(State(handle): State<OverwatchHandle>, Json(tx): Json<Tx>) -> Response
where
    Tx: Transaction + Clone + Debug + Hash + Serialize + DeserializeOwned + Send + Sync + 'static,
    <Tx as Transaction>::Hash: std::cmp::Ord + Debug + Send + Sync + 'static,
{
    make_request_and_return_response!(mempool::add_tx::<
        NetworkBackend,
        MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
        Tx,
        <Tx as Transaction>::Hash,
    >(&handle, tx, Transaction::hash))
}

#[utoipa::path(
    post,
    path = "/mempool/add/blobinfo",
    responses(
        (status = 200, description = "Add blob info to the mempool"),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn add_blob_info<B>(
    State(handle): State<OverwatchHandle>,
    Json(blob_info): Json<B>,
) -> Response
where
    B: DispersedBlobInfo
        + Clone
        + Debug
        + Hash
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    <B as DispersedBlobInfo>::BlobId: std::cmp::Ord + Clone + Debug + Hash + Send + Sync + 'static,
{
    make_request_and_return_response!(mempool::add_blob_info::<
        NetworkBackend,
        MempoolNetworkAdapter<B, <B as DispersedBlobInfo>::BlobId>,
        B,
        <B as DispersedBlobInfo>::BlobId,
    >(&handle, blob_info, DispersedBlobInfo::blob_id))
}

#[utoipa::path(
    get,
    path = "/metrics",
    responses(
        (status = 200, description = "Get all metrics"),
        (status = 500, description = "Internal server error", body = String),
    )
)]
async fn get_metrics(State(handle): State<OverwatchHandle>) -> Response {
    match metrics::gather(&handle).await {
        Ok(encoded_metrics) => Response::builder()
            .status(StatusCode::OK)
            .header(
                CONTENT_TYPE,
                HeaderValue::from_static("text/plain; version=0.0.4"),
            )
            .body(Body::from(encoded_metrics))
            .unwrap()
            .into_response(),
        Err(e) => axum::response::IntoResponse::into_response((
            hyper::StatusCode::INTERNAL_SERVER_ERROR,
            e.to_string(),
        )),
    }
}
