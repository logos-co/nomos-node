use std::{error::Error, fmt::Debug, hash::Hash};

use axum::{http::HeaderValue, routing, Router, Server};
use hyper::header::{CONTENT_TYPE, USER_AGENT};
use nomos_api::Backend;
use nomos_core::{
    da::{
        blob::{info::DispersedBlobInfo, metadata::Metadata, Blob},
        DaVerifier as CoreDaVerifier,
    },
    header::HeaderId,
    tx::Transaction,
};
use nomos_da_network_core::SubnetworkId;
use nomos_da_sampling::backend::DaSamplingServiceBackend;
use nomos_da_verifier::backend::VerifierBackend;
use nomos_libp2p::PeerId;
use nomos_mempool::{tx::service::openapi::Status, MempoolMetrics};
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

use super::handlers::{
    add_blob, add_blob_info, add_tx, block, cl_metrics, cl_status, cryptarchia_headers,
    cryptarchia_info, da_get_commitments, da_get_light_blob, get_range, libp2p_info,
};
use crate::api::paths;

/// Configuration for the Http Server
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct AxumBackendSettings {
    /// Socket where the server will be listening on for incoming requests.
    pub address: std::net::SocketAddr,
    /// Allowed origins for this server deployment requests.
    pub cors_origins: Vec<String>,
}

pub struct AxumBackend<
    DaAttestation,
    DaBlob,
    DaBlobInfo,
    Membership,
    DaVerifiedBlobInfo,
    DaVerifierBackend,
    DaVerifierNetwork,
    DaVerifierStorage,
    Tx,
    DaStorageSerializer,
    SamplingBackend,
    SamplingNetworkAdapter,
    SamplingRng,
    SamplingStorage,
    TimeBackend,
    ApiAdapter,
    const SIZE: usize,
> {
    settings: AxumBackendSettings,
    _attestation: core::marker::PhantomData<DaAttestation>,
    _blob: core::marker::PhantomData<DaBlob>,
    _certificate: core::marker::PhantomData<DaBlobInfo>,
    _membership: core::marker::PhantomData<Membership>,
    _vid: core::marker::PhantomData<DaVerifiedBlobInfo>,
    _verifier_backend: core::marker::PhantomData<DaVerifierBackend>,
    _verifier_network: core::marker::PhantomData<DaVerifierNetwork>,
    _verifier_storage: core::marker::PhantomData<DaVerifierStorage>,
    _tx: core::marker::PhantomData<Tx>,
    _storage_serde: core::marker::PhantomData<DaStorageSerializer>,
    _sampling_backend: core::marker::PhantomData<SamplingBackend>,
    _sampling_network_adapter: core::marker::PhantomData<SamplingNetworkAdapter>,
    _sampling_rng: core::marker::PhantomData<SamplingRng>,
    _sampling_storage: core::marker::PhantomData<SamplingStorage>,
    _time_backend: core::marker::PhantomData<TimeBackend>,
    _api_adapter: core::marker::PhantomData<ApiAdapter>,
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
        DaAttestation,
        DaBlob,
        DaBlobInfo,
        Membership,
        DaVerifiedBlobInfo,
        DaVerifierBackend,
        DaVerifierNetwork,
        DaVerifierStorage,
        Tx,
        DaStorageSerializer,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SamplingStorage,
        TimeBackend,
        ApiAdapter,
        const SIZE: usize,
    > Backend
    for AxumBackend<
        DaAttestation,
        DaBlob,
        DaBlobInfo,
        Membership,
        DaVerifiedBlobInfo,
        DaVerifierBackend,
        DaVerifierNetwork,
        DaVerifierStorage,
        Tx,
        DaStorageSerializer,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SamplingStorage,
        TimeBackend,
        ApiAdapter,
        SIZE,
    >
where
    DaAttestation: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    DaBlob: Blob + Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    <DaBlob as Blob>::BlobId:
        AsRef<[u8]> + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    <DaBlob as Blob>::ColumnIndex: AsRef<[u8]> + DeserializeOwned + Send + Sync + 'static,
    DaBlob::LightBlob: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    DaBlob::SharedCommitments: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    DaBlobInfo: DispersedBlobInfo<BlobId = [u8; 32]>
        + Clone
        + Debug
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    <DaBlobInfo as DispersedBlobInfo>::BlobId: Clone + Send + Sync,
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
    DaVerifiedBlobInfo: DispersedBlobInfo<BlobId = [u8; 32]>
        + From<DaBlobInfo>
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
    <DaVerifiedBlobInfo as DispersedBlobInfo>::BlobId: Debug + Clone + Ord + Hash,
    <DaVerifiedBlobInfo as Metadata>::AppId:
        AsRef<[u8]> + Clone + Serialize + DeserializeOwned + Send + Sync,
    <DaVerifiedBlobInfo as Metadata>::Index:
        AsRef<[u8]> + Clone + Serialize + DeserializeOwned + PartialOrd + Send + Sync,
    DaVerifierBackend: VerifierBackend + CoreDaVerifier<DaBlob = DaBlob> + Send + Sync + 'static,
    <DaVerifierBackend as VerifierBackend>::Settings: Clone,
    <DaVerifierBackend as CoreDaVerifier>::Error: Error,
    DaVerifierNetwork: nomos_da_verifier::network::NetworkAdapter + Send + Sync + 'static,
    DaVerifierStorage: nomos_da_verifier::storage::DaStorageAdapter + Send + Sync + 'static,
    Tx: Transaction
        + Clone
        + Debug
        + Eq
        + Hash
        + Serialize
        + for<'de> Deserialize<'de>
        + Send
        + Sync
        + 'static,
    <Tx as nomos_core::tx::Transaction>::Hash:
        Serialize + for<'de> Deserialize<'de> + std::cmp::Ord + Debug + Send + Sync + 'static,
    DaStorageSerializer: StorageSerde + Send + Sync + 'static,
    <DaStorageSerializer as StorageSerde>::Error: Send + Sync,
    SamplingRng: SeedableRng + RngCore + Send + 'static,
    SamplingBackend: DaSamplingServiceBackend<
            SamplingRng,
            BlobId = <DaVerifiedBlobInfo as DispersedBlobInfo>::BlobId,
        > + Send
        + 'static,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter + Send + 'static,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter + Send + 'static,
    DaVerifierNetwork::Settings: Clone,
    TimeBackend: nomos_time::backends::TimeBackend + Send + 'static,
    TimeBackend::Settings: Clone + Send + Sync,
    ApiAdapter: nomos_da_sampling::api::ApiAdapter + Send + Sync + 'static,
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
            _verifier_network: core::marker::PhantomData,
            _verifier_storage: core::marker::PhantomData,
            _tx: core::marker::PhantomData,
            _storage_serde: core::marker::PhantomData,
            _sampling_backend: core::marker::PhantomData,
            _sampling_network_adapter: core::marker::PhantomData,
            _sampling_rng: core::marker::PhantomData,
            _sampling_storage: core::marker::PhantomData,
            _time_backend: core::marker::PhantomData,
            _api_adapter: core::marker::PhantomData,
        })
    }

    #[expect(clippy::too_many_lines, reason = "TODO: Address this at some point.")]
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
            .route(paths::CL_METRICS, routing::get(cl_metrics::<Tx>))
            .route(paths::CL_STATUS, routing::post(cl_status::<Tx>))
            .route(
                paths::CRYPTARCHIA_INFO,
                routing::get(
                    cryptarchia_info::<
                        Tx,
                        DaStorageSerializer,
                        SamplingBackend,
                        SamplingNetworkAdapter,
                        SamplingRng,
                        SamplingStorage,
                        DaVerifierBackend,
                        DaVerifierNetwork,
                        DaVerifierStorage,
                        TimeBackend,
                        ApiAdapter,
                        SIZE,
                    >,
                ),
            )
            .route(
                paths::CRYPTARCHIA_HEADERS,
                routing::get(
                    cryptarchia_headers::<
                        Tx,
                        DaStorageSerializer,
                        SamplingBackend,
                        SamplingNetworkAdapter,
                        SamplingRng,
                        SamplingStorage,
                        DaVerifierBackend,
                        DaVerifierNetwork,
                        DaVerifierStorage,
                        TimeBackend,
                        ApiAdapter,
                        SIZE,
                    >,
                ),
            )
            .route(
                paths::DA_ADD_BLOB,
                routing::post(
                    add_blob::<
                        DaAttestation,
                        DaBlob,
                        Membership,
                        DaVerifierBackend,
                        DaStorageSerializer,
                    >,
                ),
            )
            .route(
                paths::DA_GET_RANGE,
                routing::post(
                    get_range::<
                        Tx,
                        DaBlobInfo,
                        DaVerifiedBlobInfo,
                        DaStorageSerializer,
                        SamplingBackend,
                        SamplingNetworkAdapter,
                        SamplingRng,
                        SamplingStorage,
                        DaVerifierBackend,
                        DaVerifierNetwork,
                        DaVerifierStorage,
                        TimeBackend,
                        ApiAdapter,
                        SIZE,
                    >,
                ),
            )
            .route(paths::NETWORK_INFO, routing::get(libp2p_info))
            .route(
                paths::STORAGE_BLOCK,
                routing::post(block::<DaStorageSerializer, Tx>),
            )
            .route(paths::MEMPOOL_ADD_TX, routing::post(add_tx::<Tx>))
            .route(
                paths::MEMPOOL_ADD_BLOB_INFO,
                routing::post(
                    add_blob_info::<
                        DaVerifiedBlobInfo,
                        SamplingBackend,
                        SamplingNetworkAdapter,
                        SamplingRng,
                        SamplingStorage,
                        DaVerifierBackend,
                        DaVerifierNetwork,
                        DaVerifierStorage,
                        ApiAdapter,
                    >,
                ),
            )
            .route(
                paths::DA_GET_SHARED_COMMITMENTS,
                routing::get(da_get_commitments::<DaStorageSerializer, DaBlob>),
            )
            .route(
                paths::DA_GET_LIGHT_BLOB,
                routing::get(da_get_light_blob::<DaStorageSerializer, DaBlob>),
            )
            .with_state(handle);

        Server::bind(&self.settings.address)
            .serve(app.into_make_service())
            .await
    }
}
