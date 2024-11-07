pub mod api;
pub mod config;

// std
// crates
use rand_chacha::ChaCha20Rng;
// internal
use api::backend::AxumBackend;
use kzgrs_backend::common::blob::DaBlob;
use nomos_api::ApiService;
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
use nomos_mix_service::backends::libp2p::Libp2pMixBackend as MixBackend;
use nomos_mix_service::network::libp2p::Libp2pAdapter as MixNetworkAdapter;
use nomos_mix_service::MixService;
use nomos_node::DispersedBlobInfo;
use nomos_node::HeaderId;
use nomos_node::MempoolNetworkAdapter;
use nomos_node::NetworkBackend;
use nomos_node::{
    BlobInfo, Cryptarchia, DaIndexer, DaMempool, DaNetworkService, DaSampling, DaVerifier,
    NetworkService, NomosDaMembership, RocksBackend, StorageService, SystemSig, Tracing, Tx,
    TxMempool, Wire, MB16,
};
use overwatch_derive::Services;
use overwatch_rs::services::handle::ServiceHandle;

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
    tracing: ServiceHandle<Tracing>,
    network: ServiceHandle<NetworkService<NetworkBackend>>,
    mix: ServiceHandle<MixService<MixBackend, MixNetworkAdapter>>,
    da_dispersal: ServiceHandle<DaDispersal>,
    da_indexer: ServiceHandle<ExecutorDaIndexer>,
    da_verifier: ServiceHandle<ExecutorDaVerifier>,
    da_sampling: ServiceHandle<ExecutorDaSampling>,
    da_network: ServiceHandle<DaNetworkService<DaNetworkExecutorBackend<NomosDaMembership>>>,
    cl_mempool: ServiceHandle<TxMempool>,
    da_mempool: ServiceHandle<DaMempool>,
    cryptarchia: ServiceHandle<ExecutorCryptarchia>,
    http: ServiceHandle<ExecutorApiService>,
    storage: ServiceHandle<StorageService<RocksBackend<Wire>>>,
    system_sig: ServiceHandle<SystemSig>,
}
