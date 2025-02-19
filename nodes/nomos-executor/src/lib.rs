pub mod api;
pub mod config;

// std
// crates
use rand_chacha::ChaCha20Rng;
// internal
use api::backend::AxumBackend;
use kzgrs_backend::common::blob::DaBlob;
use nomos_api::ApiService;
use nomos_blend_service::backends::libp2p::Libp2pBlendBackend as BlendBackend;
use nomos_blend_service::network::libp2p::Libp2pAdapter as BlendNetworkAdapter;
use nomos_blend_service::BlendService;
use nomos_da_dispersal::adapters::mempool::kzgrs::KzgrsMempoolAdapter;
use nomos_da_dispersal::adapters::network::libp2p::Libp2pNetworkAdapter as DispersalNetworkAdapter;
use nomos_da_dispersal::backend::kzgrs::DispersalKZGRSBackend;
use nomos_da_dispersal::DispersalService;
use nomos_da_network_service::backends::libp2p::executor::DaNetworkExecutorBackend;
use nomos_da_sampling::backend::kzgrs::KzgrsSamplingBackend;
use nomos_da_sampling::storage::adapters::rocksdb::RocksAdapter as SamplingStorageAdapter;
use nomos_da_verifier::backend::kzgrs::KzgrsDaVerifier;
use nomos_da_verifier::network::adapters::executor::Libp2pAdapter as VerifierNetworkAdapter;
use nomos_mempool::backend::mockpool::MockPool;
use nomos_node::DispersedBlobInfo;
use nomos_node::HeaderId;
use nomos_node::MempoolNetworkAdapter;
use nomos_node::NetworkBackend;
use nomos_node::{
    BlobInfo, Cryptarchia, DaIndexer, DaMempool, DaNetworkService, DaSampling, DaVerifier,
    NetworkService, NomosDaMembership, RocksBackend, StorageService, SystemSig, Tx, TxMempool,
    Wire, MB16,
};
use overwatch_derive::Services;
use overwatch_rs::OpaqueServiceHandle;

pub type ExecutorApiService = ApiService<
    AxumBackend<
        (),
        DaBlob,
        BlobInfo,
        NomosDaMembership,
        BlobInfo,
        KzgrsDaVerifier,
        Tx,
        Wire,
        DispersalKZGRSBackend<DispersalNetworkAdapter<NomosDaMembership>, DispersalMempoolAdapter>,
        DispersalNetworkAdapter<NomosDaMembership>,
        DispersalMempoolAdapter,
        kzgrs_backend::dispersal::Metadata,
        KzgrsSamplingBackend<ChaCha20Rng>,
        nomos_da_sampling::network::adapters::executor::Libp2pAdapter<NomosDaMembership>,
        ChaCha20Rng,
        SamplingStorageAdapter<DaBlob, Wire>,
        MB16,
    >,
>;

pub type DispersalMempoolAdapter = KzgrsMempoolAdapter<
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    KzgrsSamplingBackend<ChaCha20Rng>,
    nomos_da_sampling::network::adapters::executor::Libp2pAdapter<NomosDaMembership>,
    ChaCha20Rng,
    SamplingStorageAdapter<DaBlob, Wire>,
>;

pub type DaDispersal = DispersalService<
    DispersalKZGRSBackend<DispersalNetworkAdapter<NomosDaMembership>, DispersalMempoolAdapter>,
    DispersalNetworkAdapter<NomosDaMembership>,
    DispersalMempoolAdapter,
    NomosDaMembership,
    kzgrs_backend::dispersal::Metadata,
>;

pub type ExecutorCryptarchia =
    Cryptarchia<nomos_da_sampling::network::adapters::executor::Libp2pAdapter<NomosDaMembership>>;

pub type ExecutorDaIndexer =
    DaIndexer<nomos_da_sampling::network::adapters::executor::Libp2pAdapter<NomosDaMembership>>;

pub type ExecutorDaSampling =
    DaSampling<nomos_da_sampling::network::adapters::executor::Libp2pAdapter<NomosDaMembership>>;

pub type ExecutorDaVerifier = DaVerifier<VerifierNetworkAdapter<NomosDaMembership>>;

#[derive(Services)]
pub struct NomosExecutor {
    #[cfg(feature = "tracing")]
    tracing: OpaqueServiceHandle<nomos_node::Tracing>,
    network: OpaqueServiceHandle<NetworkService<NetworkBackend>>,
    blend: OpaqueServiceHandle<BlendService<BlendBackend, BlendNetworkAdapter>>,
    da_dispersal: OpaqueServiceHandle<DaDispersal>,
    da_indexer: OpaqueServiceHandle<ExecutorDaIndexer>,
    da_verifier: OpaqueServiceHandle<ExecutorDaVerifier>,
    da_sampling: OpaqueServiceHandle<ExecutorDaSampling>,
    da_network: OpaqueServiceHandle<DaNetworkService<DaNetworkExecutorBackend<NomosDaMembership>>>,
    cl_mempool: OpaqueServiceHandle<TxMempool>,
    da_mempool: OpaqueServiceHandle<DaMempool>,
    cryptarchia: OpaqueServiceHandle<ExecutorCryptarchia>,
    http: OpaqueServiceHandle<ExecutorApiService>,
    storage: OpaqueServiceHandle<StorageService<RocksBackend<Wire>>>,
    system_sig: OpaqueServiceHandle<SystemSig>,
}
