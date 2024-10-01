// std
use std::error::Error;
use std::{fmt::Debug, hash::Hash};
// crates
use axum::{http::HeaderValue, routing, Router, Server};
use hyper::header::{CONTENT_TYPE, USER_AGENT};
use nomos_api::Backend;
use nomos_core::da::blob::info::DispersedBlobInfo;
use nomos_core::da::blob::metadata::Metadata;
use nomos_core::da::DaVerifier as CoreDaVerifier;
use nomos_core::{da::blob::Blob, header::HeaderId, tx::Transaction};
use nomos_da_network_core::SubnetworkId;
use nomos_da_sampling::backend::DaSamplingServiceBackend;
use nomos_da_verifier::backend::VerifierBackend;
use nomos_libp2p::PeerId;
use nomos_mempool::{tx::service::openapi::Status, MempoolMetrics};
use nomos_node::api::handlers::{
    add_blob, add_blob_info, add_tx, block, cl_metrics, cl_status, cryptarchia_headers,
    cryptarchia_info, get_metrics, get_range, libp2p_info,
};
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
    SamplingStorage,
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
    _sampling_storage: core::marker::PhantomData<SamplingStorage>,
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
        SamplingStorage,
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
        SamplingStorage,
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
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = <V as DispersedBlobInfo>::BlobId>
        + Send
        + 'static,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter + Send + 'static,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter + Send + 'static,
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
            _sampling_storage: core::marker::PhantomData,
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
                        SamplingStorage,
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
                        SamplingStorage,
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
                        SamplingStorage,
                        SIZE,
                    >,
                ),
            )
            .route("/network/info", routing::get(libp2p_info))
            .route("/storage/block", routing::post(block::<S, T>))
            .route("/mempool/add/tx", routing::post(add_tx::<T>))
            .route(
                "/mempool/add/blobinfo",
                routing::post(
                    add_blob_info::<
                        V,
                        SamplingBackend,
                        SamplingNetworkAdapter,
                        SamplingRng,
                        SamplingStorage,
                    >,
                ),
            )
            .route("/metrics", routing::get(get_metrics))
            .with_state(handle);

        Server::bind(&self.settings.address)
            .serve(app.into_make_service())
            .await
    }
}
