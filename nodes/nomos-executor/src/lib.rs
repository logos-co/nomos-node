pub mod api;
pub mod config;

use api::backend::AxumBackend;
use kzgrs_backend::common::blob::DaBlob;
use nomos_api::ApiService;
use nomos_blend_service::{
    backends::libp2p::Libp2pBlendBackend as BlendBackend,
    network::libp2p::Libp2pAdapter as BlendNetworkAdapter, BlendService,
};
use nomos_da_dispersal::{
    adapters::{
        mempool::kzgrs::KzgrsMempoolAdapter,
        network::libp2p::Libp2pNetworkAdapter as DispersalNetworkAdapter,
    },
    backend::kzgrs::DispersalKZGRSBackend,
    DispersalService,
};
use nomos_da_network_service::backends::libp2p::executor::DaNetworkExecutorBackend;
use nomos_da_sampling::{
    api::http::HttApiAdapter, backend::kzgrs::KzgrsSamplingBackend,
    storage::adapters::rocksdb::RocksAdapter as SamplingStorageAdapter,
};
use nomos_da_verifier::{
    backend::kzgrs::KzgrsDaVerifier,
    network::adapters::executor::Libp2pAdapter as VerifierNetworkAdapter,
    storage::adapters::rocksdb::RocksAdapter as VerifierStorageAdapter,
};
use nomos_mempool::backend::mockpool::MockPool;
use nomos_node::{
    BlobInfo, Cryptarchia, DaIndexer, DaMempool, DaNetworkService, DaSampling, DaVerifier,
    DispersedBlobInfo, HeaderId, MempoolNetworkAdapter, NetworkBackend, NetworkService,
    NomosDaMembership, NomosTimeService, RocksBackend, StorageService, SystemSig, Tx, TxMempool,
    Wire, MB16,
};
use overwatch::OpaqueServiceHandle;
use overwatch_derive::Services;
use rand_chacha::ChaCha20Rng;

pub type ExecutorApiService = ApiService<
    AxumBackend<
        (),
        DaBlob,
        BlobInfo,
        NomosDaMembership,
        BlobInfo,
        KzgrsDaVerifier,
        VerifierNetworkAdapter<NomosDaMembership>,
        VerifierStorageAdapter<DaBlob, Wire>,
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
        nomos_time::backends::system_time::SystemTimeBackend,
        HttApiAdapter<NomosDaMembership>,
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
    KzgrsDaVerifier,
    VerifierNetworkAdapter<NomosDaMembership>,
    VerifierStorageAdapter<DaBlob, Wire>,
    HttApiAdapter<NomosDaMembership>,
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
    time: OpaqueServiceHandle<NomosTimeService>,
    http: OpaqueServiceHandle<ExecutorApiService>,
    storage: OpaqueServiceHandle<StorageService<RocksBackend<Wire>>>,
    system_sig: OpaqueServiceHandle<SystemSig>,
}
