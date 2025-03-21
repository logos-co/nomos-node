use std::{
    error::Error,
    fmt::{Debug, Display},
    hash::Hash,
};

use axum::{
    body::StreamBody,
    extract::{Query, State},
    response::{IntoResponse, Response},
    Json,
};
use http::{header, StatusCode};
use nomos_api::http::{
    cl, consensus,
    da::{self, PeerMessagesFactory},
    da_shares, libp2p, mempool, storage,
};
use nomos_core::{
    da::{
        blob::{info::DispersedBlobInfo, metadata::Metadata, LightShare, Share},
        BlobId, DaVerifier as CoreDaVerifier,
    },
    header::HeaderId,
    tx::Transaction,
};
use nomos_da_messages::http::da::{
    DASharesCommitmentsRequest, DaSamplingRequest, GetRangeReq, GetSharesRequest,
};
use nomos_da_network_core::SubnetworkId;
use nomos_da_network_service::backends::NetworkBackend;
use nomos_da_sampling::backend::DaSamplingServiceBackend;
use nomos_da_verifier::backend::VerifierBackend;
use nomos_libp2p::PeerId;
use nomos_mempool::network::adapters::libp2p::Libp2pAdapter as MempoolNetworkAdapter;
use nomos_network::backends::libp2p::Libp2p as Libp2pNetworkBackend;
use nomos_storage::backends::StorageSerde;
use overwatch::overwatch::handle::OverwatchHandle;
use rand::{RngCore, SeedableRng};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use subnetworks_assignations::MembershipHandler;

use super::paths;

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
pub async fn cl_metrics<T, RuntimeServiceId>(
    State(handle): State<OverwatchHandle<RuntimeServiceId>>,
) -> Response
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
        std::cmp::Ord + Debug + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static,
{
    make_request_and_return_response!(cl::cl_mempool_metrics::<T, RuntimeServiceId>(&handle))
}

#[utoipa::path(
    post,
    path = paths::CL_STATUS,
    responses(
        (status = 200, description = "Query the mempool status of the cl service", body = Vec<<T as Transaction>::Hash>),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn cl_status<T, RuntimeServiceId>(
    State(handle): State<OverwatchHandle<RuntimeServiceId>>,
    Json(items): Json<Vec<<T as Transaction>::Hash>>,
) -> Response
where
    T: Transaction + Clone + Debug + Hash + Serialize + DeserializeOwned + Send + Sync + 'static,
    <T as nomos_core::tx::Transaction>::Hash:
        Serialize + DeserializeOwned + std::cmp::Ord + Debug + Send + Sync + 'static,
{
    make_request_and_return_response!(cl::cl_mempool_status::<T, RuntimeServiceId>(&handle, items))
}
#[derive(Deserialize)]
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
    DaVerifierBackend,
    DaVerifierNetwork,
    DaVerifierStorage,
    TimeBackend,
    ApiAdapter,
    RuntimeServiceId,
    const SIZE: usize,
>(
    State(handle): State<OverwatchHandle<RuntimeServiceId>>,
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
    <Tx as Transaction>::Hash:
        std::cmp::Ord + Debug + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static,
    SS: StorageSerde + Send + Sync + 'static,
    SamplingRng: SeedableRng + RngCore,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = BlobId> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Share: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter<RuntimeServiceId>,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter<RuntimeServiceId>,
    DaVerifierStorage: nomos_da_verifier::storage::DaStorageAdapter<RuntimeServiceId>,
    DaVerifierBackend: nomos_da_verifier::backend::VerifierBackend + Send + 'static,
    DaVerifierBackend::Settings: Clone,
    DaVerifierNetwork: nomos_da_verifier::network::NetworkAdapter<RuntimeServiceId>,
    DaVerifierNetwork::Settings: Clone,
    TimeBackend: nomos_time::backends::TimeBackend,
    TimeBackend::Settings: Clone + Send + Sync,
    ApiAdapter: nomos_da_sampling::api::ApiAdapter + Send + Sync,
{
    make_request_and_return_response!(consensus::cryptarchia_info::<
        Tx,
        SS,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SamplingStorage,
        DaVerifierBackend,
        DaVerifierNetwork,
        DaVerifierStorage,
        TimeBackend,
        ApiAdapter,
        RuntimeServiceId,
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
    DaVerifierBackend,
    DaVerifierNetwork,
    DaVerifierStorage,
    TimeBackend,
    ApiAdapter,
    RuntimeServiceId,
    const SIZE: usize,
>(
    State(store): State<OverwatchHandle<RuntimeServiceId>>,
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
    <Tx as Transaction>::Hash:
        std::cmp::Ord + Debug + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static,
    SS: StorageSerde + Send + Sync + 'static,
    SamplingRng: SeedableRng + RngCore,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = BlobId> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Share: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter<RuntimeServiceId>,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter<RuntimeServiceId>,
    DaVerifierStorage: nomos_da_verifier::storage::DaStorageAdapter<RuntimeServiceId>,
    DaVerifierBackend: nomos_da_verifier::backend::VerifierBackend + Send + 'static,
    DaVerifierBackend::Settings: Clone,
    DaVerifierNetwork: nomos_da_verifier::network::NetworkAdapter<RuntimeServiceId>,
    DaVerifierNetwork::Settings: Clone,
    TimeBackend: nomos_time::backends::TimeBackend,
    TimeBackend::Settings: Clone + Send + Sync,
    ApiAdapter: nomos_da_sampling::api::ApiAdapter + Send + Sync,
{
    let CryptarchiaInfoQuery { from, to } = query;
    make_request_and_return_response!(consensus::cryptarchia_headers::<
        Tx,
        SS,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SamplingStorage,
        DaVerifierBackend,
        DaVerifierNetwork,
        DaVerifierStorage,
        TimeBackend,
        ApiAdapter,
        RuntimeServiceId,
        SIZE,
    >(&store, from, to))
}

#[utoipa::path(
    post,
    path = paths::DA_ADD_SHARE,
    responses(
        (status = 200, description = "Share to be published received"),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn add_share<A, S, M, VB, SS, RuntimeServiceId>(
    State(handle): State<OverwatchHandle<RuntimeServiceId>>,
    Json(share): Json<S>,
) -> Response
where
    A: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    S: Share + Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    <S as Share>::BlobId: AsRef<[u8]> + Send + Sync + 'static,
    <S as Share>::ShareIndex:
        AsRef<[u8]> + Serialize + DeserializeOwned + Hash + Eq + Send + Sync + 'static,
    <S as Share>::LightShare: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    <S as Share>::SharesCommitments: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    M: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
    VB: VerifierBackend + CoreDaVerifier<DaShare = S>,
    <VB as VerifierBackend>::Settings: Clone,
    <VB as CoreDaVerifier>::Error: Error,
    SS: StorageSerde + Send + Sync + 'static,
{
    make_request_and_return_response!(da::add_share::<A, S, M, VB, SS, RuntimeServiceId>(
        &handle, share
    ))
}

#[utoipa::path(
    post,
    path = paths::DA_GET_RANGE,
    responses(
        (status = 200, description = "Range of blobs", body = Vec<([u8;8], Vec<DaShare>)>),
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
    DaVerifierBackend,
    DaVerifierNetwork,
    DaVerifierStorage,
    TimeBackend,
    ApiAdapter,
    RuntimeServiceId,
    const SIZE: usize,
>(
    State(handle): State<OverwatchHandle<RuntimeServiceId>>,
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
    <Tx as Transaction>::Hash:
        std::cmp::Ord + Debug + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static,
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
    SamplingBackend::Share: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter<RuntimeServiceId>,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter<RuntimeServiceId>,
    DaVerifierStorage: nomos_da_verifier::storage::DaStorageAdapter<RuntimeServiceId>,
    DaVerifierBackend: nomos_da_verifier::backend::VerifierBackend + Send + 'static,
    DaVerifierBackend::Settings: Clone,
    DaVerifierNetwork: nomos_da_verifier::network::NetworkAdapter<RuntimeServiceId>,
    DaVerifierNetwork::Settings: Clone,
    TimeBackend: nomos_time::backends::TimeBackend,
    TimeBackend::Settings: Clone + Send + Sync,
    ApiAdapter: nomos_da_sampling::api::ApiAdapter + Send + Sync,
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
        DaVerifierBackend,
        DaVerifierNetwork,
        DaVerifierStorage,
        TimeBackend,
        ApiAdapter,
        RuntimeServiceId
        SIZE,
    >(&handle, app_id, range))
}

#[utoipa::path(
    post,
    path = paths::DA_BLOCK_PEER,
    responses(
        (status = 200, description = "Block a peer", body = bool),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn block_peer<Backend, RuntimeServiceId>(
    State(handle): State<OverwatchHandle<RuntimeServiceId>>,
    Json(peer_id): Json<PeerId>,
) -> Response
where
    Backend: NetworkBackend + Send + 'static,
    Backend::Message: PeerMessagesFactory,
{
    make_request_and_return_response!(da::block_peer::<Backend>(&handle, peer_id))
}

#[utoipa::path(
    post,
    path = paths::DA_UNBLOCK_PEER,
    responses(
        (status = 200, description = "Unblock a peer", body = bool),
        (status = 500, description = "Internal server error", body = String),
    )
)]

pub async fn unblock_peer<Backend, RuntimeServiceId>(
    State(handle): State<OverwatchHandle<RuntimeServiceId>>,
    Json(peer_id): Json<PeerId>,
) -> Response
where
    Backend: NetworkBackend + Send + 'static,
    Backend::Message: PeerMessagesFactory,
{
    make_request_and_return_response!(da::unblock_peer::<Backend>(&handle, peer_id))
}

#[utoipa::path(
    get,
    path = paths::DA_BLACKLISTED_PEERS,
    responses(
        (status = 200, description = "Get the blacklisted peers", body = Vec<PeerId>),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn blacklisted_peers<Backend, RuntimeServiceId>(
    State(handle): State<OverwatchHandle<RuntimeServiceId>>,
) -> Response
where
    Backend: NetworkBackend + Send + 'static,
    Backend::Message: PeerMessagesFactory,
{
    make_request_and_return_response!(da::blacklisted_peers::<Backend>(&handle))
}

#[utoipa::path(
    get,
    path = paths::NETWORK_INFO,
    responses(
        (status = 200, description = "Query the network information", body = nomos_network::backends::libp2p::Libp2pInfo),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn libp2p_info<RuntimeServiceId>(
    State(handle): State<OverwatchHandle<RuntimeServiceId>>,
) -> Response
where
    RuntimeServiceId: Debug,
{
    make_request_and_return_response!(libp2p::libp2p_info(&handle))
}

#[utoipa::path(
    get,
    path = paths::STORAGE_BLOCK,
    responses(
        (status = 200, description = "Get the block by block id", body = HeaderId),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn block<S, Tx, RuntimeServiceId>(
    State(handle): State<OverwatchHandle<RuntimeServiceId>>,
    Json(id): Json<HeaderId>,
) -> Response
where
    Tx: serde::Serialize + serde::de::DeserializeOwned + Clone + Eq + core::hash::Hash,
    S: StorageSerde + Send + Sync + 'static,
    RuntimeServiceId: Debug + Display,
{
    make_request_and_return_response!(storage::block_req::<S, Tx, RuntimeServiceId>(&handle, id))
}

#[utoipa::path(
    get,
    path = paths::DA_GET_SHARES_COMMITMENTS,
    responses(
        (status = 200, description = "Request the commitments for an specific `BlobId`", body = DASharesCommitmentsRequest<DaShare>),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn da_get_commitments<StorageOp, DaShare, RuntimeServiceId>(
    State(handle): State<OverwatchHandle<RuntimeServiceId>>,
    Json(req): Json<DASharesCommitmentsRequest<DaShare>>,
) -> Response
where
    DaShare: Share,
    <DaShare as Share>::BlobId:
        AsRef<[u8]> + Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    <DaShare as Share>::SharesCommitments:
        serde::Serialize + DeserializeOwned + Send + Sync + 'static,
    StorageOp: StorageSerde + Send + Sync + 'static,
    <StorageOp as StorageSerde>::Error: Send + Sync,
{
    make_request_and_return_response!(storage::get_shared_commitments::<StorageOp, DaShare>(
        &handle,
        req.blob_id
    ))
}

#[utoipa::path(
    get,
    path = paths::DA_GET_LIGHT_SHARE,
    responses(
        (status = 200, description = "Get blob by blob id", body = DaSamplingRequest<DaShare>),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn da_get_light_share<StorageOp, DaShare, RuntimeServiceId>(
    State(handle): State<OverwatchHandle<RuntimeServiceId>>,
    Json(request): Json<DaSamplingRequest<DaShare>>,
) -> Response
where
    DaShare: Share,
    <DaShare as Share>::BlobId: AsRef<[u8]> + DeserializeOwned + Clone + Send + Sync + 'static,
    <DaShare as Share>::ShareIndex: AsRef<[u8]> + DeserializeOwned + Send + Sync + 'static,
    <DaShare as Share>::LightShare: Serialize + DeserializeOwned + Send + Sync + 'static,
    StorageOp: StorageSerde + Send + Sync + 'static,
    <StorageOp as StorageSerde>::Error: Send + Sync,
{
    make_request_and_return_response!(storage::get_light_share::<StorageOp, DaShare>(
        &handle,
        request.blob_id,
        request.share_idx
    ))
}

#[utoipa::path(
    get,
    path = paths::DA_GET_LIGHT_SHARE,
    responses(
        (status = 200, description = "Request shares for a blob", body = GetSharesRequest<DaBlob>),
        (status = 500, description = "Internal server error", body = StreamBody),
    )
)]
pub async fn da_get_shares<StorageOp, DaShare, RuntimeServiceId>(
    State(handle): State<OverwatchHandle<RuntimeServiceId>>,
    Json(request): Json<GetSharesRequest<DaShare>>,
) -> Response
where
    DaShare: Share + 'static,
    <DaShare as Share>::BlobId: AsRef<[u8]> + DeserializeOwned + Clone + Send + Sync + 'static,
    <DaShare as Share>::ShareIndex: serde::Serialize + DeserializeOwned + Send + Sync + Eq + Hash,
    <DaShare as Share>::LightShare: LightShare<ShareIndex = <DaShare as Share>::ShareIndex>
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    StorageOp: StorageSerde + Send + Sync + 'static,
    <StorageOp as StorageSerde>::Error: Send + Sync,
    <DaShare as Share>::LightShare: LightShare<ShareIndex = <DaShare as Share>::ShareIndex>,
    <DaShare as Share>::ShareIndex: AsRef<[u8]> + 'static,
{
    match da_shares::get_shares::<StorageOp, DaShare>(
        &handle,
        request.blob_id,
        request.requested_shares,
        request.filter_shares,
        request.return_available,
    )
    .await
    {
        Ok(shares) => {
            let body = StreamBody::new(shares);
            IntoResponse::into_response(([(header::CONTENT_TYPE, "application/x-ndjson")], body))
        }
        Err(e) => IntoResponse::into_response((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

#[utoipa::path(
    post,
    path = paths::MEMPOOL_ADD_TX,
    responses(
        (status = 200, description = "Add transaction to the mempool"),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn add_tx<Tx, RuntimeServiceId>(
    State(handle): State<OverwatchHandle<RuntimeServiceId>>,
    Json(tx): Json<Tx>,
) -> Response
where
    Tx: Transaction + Clone + Debug + Hash + Serialize + DeserializeOwned + Send + Sync + 'static,
    <Tx as Transaction>::Hash:
        std::cmp::Ord + Debug + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static,
{
    make_request_and_return_response!(mempool::add_tx::<
        Libp2pNetworkBackend,
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
pub async fn add_blob_info<
    B,
    SamplingBackend,
    SamplingAdapter,
    SamplingRng,
    SamplingStorage,
    DaVerifierBackend,
    DaVerifierNetwork,
    DaVerifierStorage,
    ApiAdapter,
    RuntimeServiceId,
>(
    State(handle): State<OverwatchHandle<RuntimeServiceId>>,
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
    <B as DispersedBlobInfo>::BlobId: std::cmp::Ord
        + Clone
        + Debug
        + Hash
        + Send
        + Sync
        + Serialize
        + for<'de> Deserialize<'de>
        + 'static,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = <B as DispersedBlobInfo>::BlobId>
        + Send
        + 'static,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Share: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingAdapter: nomos_da_sampling::network::NetworkAdapter<RuntimeServiceId> + Send + 'static,
    SamplingRng: SeedableRng + RngCore + Send + 'static,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter<RuntimeServiceId>,
    DaVerifierStorage: nomos_da_verifier::storage::DaStorageAdapter<RuntimeServiceId>,
    DaVerifierBackend: nomos_da_verifier::backend::VerifierBackend + Send + 'static,
    DaVerifierBackend::Settings: Clone,
    DaVerifierNetwork: nomos_da_verifier::network::NetworkAdapter<RuntimeServiceId>,
    DaVerifierNetwork::Settings: Clone,
    ApiAdapter: nomos_da_sampling::api::ApiAdapter + Send + Sync,
{
    make_request_and_return_response!(mempool::add_blob_info::<
        Libp2pNetworkBackend,
        MempoolNetworkAdapter<B, <B as DispersedBlobInfo>::BlobId>,
        B,
        <B as DispersedBlobInfo>::BlobId,
        SamplingBackend,
        SamplingAdapter,
        SamplingRng,
        SamplingStorage,
        DaVerifierBackend,
        DaVerifierNetwork,
        DaVerifierStorage,
        ApiAdapter,
        RuntimeServiceId,
    >(&handle, blob_info, DispersedBlobInfo::blob_id))
}
