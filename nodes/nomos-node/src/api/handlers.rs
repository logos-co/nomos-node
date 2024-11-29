// std
use std::error::Error;
use std::ops::Range;
use std::{fmt::Debug, hash::Hash};
// crates
use axum::{
    extract::{Query, State},
    response::Response,
    Json,
};
use rand::{RngCore, SeedableRng};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
// internal
use super::paths;
use nomos_api::http::{cl, consensus, da, libp2p, mempool, storage};
use nomos_core::da::blob::info::DispersedBlobInfo;
use nomos_core::da::blob::metadata::Metadata;
use nomos_core::da::{BlobId, DaVerifier as CoreDaVerifier};
use nomos_core::{da::blob::Blob, header::HeaderId, tx::Transaction};
use nomos_da_network_core::SubnetworkId;
use nomos_da_sampling::backend::DaSamplingServiceBackend;
use nomos_da_verifier::backend::VerifierBackend;
use nomos_libp2p::PeerId;
use nomos_mempool::network::adapters::libp2p::Libp2pAdapter as MempoolNetworkAdapter;
use nomos_network::backends::libp2p::Libp2p as NetworkBackend;
use nomos_storage::backends::StorageSerde;
use overwatch_rs::overwatch::handle::OverwatchHandle;
use subnetworks_assignations::MembershipHandler;

#[macro_export]
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
    path = paths::CL_METRICS,
    responses(
        (status = 200, description = "Get the mempool metrics of the cl service", body = MempoolMetrics),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn cl_metrics<T>(State(handle): State<OverwatchHandle>) -> Response
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
    path = paths::CL_STATUS,
    responses(
        (status = 200, description = "Query the mempool status of the cl service", body = Vec<<T as Transaction>::Hash>),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn cl_status<T>(
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
pub struct CryptarchiaInfoQuery {
    from: Option<HeaderId>,
    to: Option<HeaderId>,
}

#[utoipa::path(
    get,
    path = paths::CRYPTARCHIA_INFO,
    responses(
        (status = 200, description = "Query consensus information", body = nomos_consensus::CryptarchiaInfo),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn cryptarchia_info<
    Tx,
    SS,
    SamplingBackend,
    SamplingNetworkAdapter,
    SamplingRng,
    SamplingStorage,
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
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = BlobId> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter,
{
    make_request_and_return_response!(consensus::cryptarchia_info::<
        Tx,
        SS,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SamplingStorage,
        SIZE,
    >(&handle))
}

#[utoipa::path(
    get,
    path = paths::CRYPTARCHIA_HEADERS,
    responses(
        (status = 200, description = "Query header ids", body = Vec<HeaderId>),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn cryptarchia_headers<
    Tx,
    SS,
    SamplingBackend,
    SamplingNetworkAdapter,
    SamplingRng,
    SamplingStorage,
    const SIZE: usize,
>(
    State(store): State<OverwatchHandle>,
    Query(query): Query<CryptarchiaInfoQuery>,
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
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = BlobId> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter,
{
    let CryptarchiaInfoQuery { from, to } = query;
    make_request_and_return_response!(consensus::cryptarchia_headers::<
        Tx,
        SS,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SamplingStorage,
        SIZE,
    >(&store, from, to))
}

#[utoipa::path(
    post,
    path = paths::DA_ADD_BLOB,
    responses(
        (status = 200, description = "Blob to be published received"),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn add_blob<A, B, M, VB, SS>(
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
pub struct GetRangeReq<V: Metadata>
where
    <V as Metadata>::AppId: Serialize + DeserializeOwned,
    <V as Metadata>::Index: Serialize + DeserializeOwned,
{
    pub app_id: <V as Metadata>::AppId,
    pub range: Range<<V as Metadata>::Index>,
}

#[utoipa::path(
    post,
    path = paths::DA_GET_RANGE,
    responses(
        (status = 200, description = "Range of blobs", body = Vec<([u8;8], Vec<DaBlob>)>),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn get_range<
    Tx,
    C,
    V,
    SS,
    SamplingBackend,
    SamplingNetworkAdapter,
    SamplingRng,
    SamplingStorage,
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
    SamplingBackend:
        DaSamplingServiceBackend<SamplingRng, BlobId = <V as DispersedBlobInfo>::BlobId> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter,
{
    make_request_and_return_response!(da::get_range::<
        Tx,
        C,
        V,
        SS,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SamplingStorage,
        SIZE,
    >(&handle, app_id, range))
}

#[utoipa::path(
    get,
    path = paths::NETWORK_INFO,
    responses(
        (status = 200, description = "Query the network information", body = nomos_network::backends::libp2p::Libp2pInfo),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn libp2p_info(State(handle): State<OverwatchHandle>) -> Response {
    make_request_and_return_response!(libp2p::libp2p_info(&handle))
}

#[utoipa::path(
    get,
    path = paths::STORAGE_BLOCK,
    responses(
        (status = 200, description = "Get the block by block id", body = Block<Tx, kzgrs_backend::dispersal::BlobInfo>),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn block<S, Tx>(
    State(handle): State<OverwatchHandle>,
    Json(id): Json<HeaderId>,
) -> Response
where
    Tx: serde::Serialize + serde::de::DeserializeOwned + Clone + Eq + core::hash::Hash,
    S: StorageSerde + Send + Sync + 'static,
{
    make_request_and_return_response!(storage::block_req::<S, Tx>(&handle, id))
}

#[utoipa::path(
    post,
    path = paths::MEMPOOL_ADD_TX,
    responses(
        (status = 200, description = "Add transaction to the mempool"),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn add_tx<Tx>(State(handle): State<OverwatchHandle>, Json(tx): Json<Tx>) -> Response
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
    path = paths::MEMPOOL_ADD_BLOB_INFO,
    responses(
        (status = 200, description = "Add blob info to the mempool"),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn add_blob_info<B, SamplingBackend, SamplingAdapter, SamplingRng, SamplingStorage>(
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
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = <B as DispersedBlobInfo>::BlobId>
        + Send
        + 'static,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingAdapter: nomos_da_sampling::network::NetworkAdapter + Send + 'static,
    SamplingRng: SeedableRng + RngCore + Send + 'static,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter,
{
    make_request_and_return_response!(mempool::add_blob_info::<
        NetworkBackend,
        MempoolNetworkAdapter<B, <B as DispersedBlobInfo>::BlobId>,
        B,
        <B as DispersedBlobInfo>::BlobId,
        SamplingBackend,
        SamplingAdapter,
        SamplingRng,
        SamplingStorage,
    >(&handle, blob_info, DispersedBlobInfo::blob_id))
}
