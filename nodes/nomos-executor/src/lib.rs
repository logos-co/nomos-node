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
use nomos_mempool::backend::mockpool::MockPool;
use nomos_node::*;
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
        nomos_da_sampling::network::adapters::libp2p::Libp2pAdapter<NomosDaMembership>,
        ChaCha20Rng,
        SamplingStorageAdapter<DaBlob, Wire>,
        MB16,
    >,
>;

pub type DispersalMempoolAdapter = KzgrsMempoolAdapter<
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    KzgrsSamplingBackend<ChaCha20Rng>,
    nomos_da_sampling::network::adapters::libp2p::Libp2pAdapter<NomosDaMembership>,
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

#[derive(Services)]
pub struct NomosExecutor {
    #[cfg(feature = "tracing")]
    logging: ServiceHandle<Logger>,
    network: ServiceHandle<NetworkService<NetworkBackend>>,
    da_dispersal: ServiceHandle<DaDispersal>,
    da_indexer: ServiceHandle<DaIndexer>,
    da_verifier: ServiceHandle<DaVerifier>,
    da_sampling: ServiceHandle<DaSampling>,
    da_network: ServiceHandle<DaNetworkService<DaNetworkExecutorBackend<NomosDaMembership>>>,
    cl_mempool: ServiceHandle<TxMempool>,
    da_mempool: ServiceHandle<DaMempool>,
    cryptarchia: ServiceHandle<Cryptarchia>,
    http: ServiceHandle<ExecutorApiService>,
    storage: ServiceHandle<StorageService<RocksBackend<Wire>>>,
    #[cfg(feature = "metrics")]
    metrics: ServiceHandle<Metrics>,
    system_sig: ServiceHandle<SystemSig>,
}
