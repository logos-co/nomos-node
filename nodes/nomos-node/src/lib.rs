pub mod api;
pub mod config;
mod tx;

use api::backend::AxumBackend;
use bytes::Bytes;
use color_eyre::eyre::Result;
pub use config::{Config, CryptarchiaArgs, HttpArgs, LogArgs, NetworkArgs};
use kzgrs_backend::common::share::DaShare;
pub use kzgrs_backend::dispersal::BlobInfo;
use nomos_api::ApiService;
pub use nomos_blend_service::{
    backends::libp2p::Libp2pBlendBackend as BlendBackend,
    network::libp2p::Libp2pAdapter as BlendNetworkAdapter, BlendService,
};
pub use nomos_core::{
    da::blob::{info::DispersedBlobInfo, select::FillSize as FillSizeWithBlobs},
    header::HeaderId,
    tx::{select::FillSize as FillSizeWithTx, Transaction},
    wire,
};
use nomos_da_indexer::{
    consensus::adapters::cryptarchia::CryptarchiaConsensusAdapter,
    storage::adapters::rocksdb::RocksAdapter as IndexerStorageAdapter, DataIndexerService,
};
pub use nomos_da_network_service::{
    backends::libp2p::validator::DaNetworkValidatorBackend, NetworkService as DaNetworkService,
};
use nomos_da_sampling::{
    api::http::HttApiAdapter, backend::kzgrs::KzgrsSamplingBackend,
    network::adapters::validator::Libp2pAdapter as SamplingLibp2pAdapter,
    storage::adapters::rocksdb::RocksAdapter as SamplingStorageAdapter, DaSamplingService,
};
use nomos_da_verifier::{
    backend::kzgrs::KzgrsDaVerifier,
    network::adapters::validator::Libp2pAdapter as VerifierNetworkAdapter,
    storage::adapters::rocksdb::RocksAdapter as VerifierStorageAdapter, DaVerifierService,
};
use nomos_mempool::{backend::mockpool::MockPool, TxMempoolService};
pub use nomos_mempool::{
    da::{service::DaMempoolService, settings::DaMempoolSettings},
    network::adapters::libp2p::{
        Libp2pAdapter as MempoolNetworkAdapter, Settings as MempoolAdapterSettings,
    },
};
pub use nomos_network::{backends::libp2p::Libp2p as NetworkBackend, NetworkService};
pub use nomos_storage::{
    backends::{
        rocksdb::{RocksBackend, RocksBackendSettings},
        StorageSerde,
    },
    StorageService,
};
pub use nomos_system_sig::SystemSig;
use nomos_time::{backends::system_time::SystemTimeBackend, TimeService};
#[cfg(feature = "tracing")]
pub use nomos_tracing_service::Tracing;
use overwatch::derive_services;
use rand_chacha::ChaCha20Rng;
use serde::{de::DeserializeOwned, Serialize};
use subnetworks_assignations::versions::v1::FillFromNodeList;
pub use tx::Tx;

/// Membership used by the DA Network service.
pub type NomosDaMembership = FillFromNodeList;

pub type NomosApiService = ApiService<
    AxumBackend<
        (),
        DaShare,
        BlobInfo,
        NomosDaMembership,
        BlobInfo,
        KzgrsDaVerifier,
        VerifierNetworkAdapter<NomosDaMembership, RuntimeServiceId>,
        VerifierStorageAdapter<DaShare, Wire>,
        Tx,
        Wire,
        KzgrsSamplingBackend<ChaCha20Rng>,
        nomos_da_sampling::network::adapters::validator::Libp2pAdapter<
            NomosDaMembership,
            RuntimeServiceId,
        >,
        ChaCha20Rng,
        SamplingStorageAdapter<DaShare, Wire>,
        SystemTimeBackend,
        HttApiAdapter<NomosDaMembership>,
        MB16,
    >,
    RuntimeServiceId,
>;

pub const CONSENSUS_TOPIC: &str = "/cryptarchia/proto";
pub const CL_TOPIC: &str = "cl";
pub const DA_TOPIC: &str = "da";
pub const MB16: usize = 1024 * 1024 * 16;

pub type Cryptarchia<SamplingAdapter> = cryptarchia_consensus::CryptarchiaConsensus<
    cryptarchia_consensus::network::adapters::libp2p::LibP2pAdapter<Tx, BlobInfo, RuntimeServiceId>,
    cryptarchia_consensus::blend::adapters::libp2p::LibP2pAdapter<
        BlendNetworkAdapter<RuntimeServiceId>,
        Tx,
        BlobInfo,
        RuntimeServiceId,
    >,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash, RuntimeServiceId>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId, RuntimeServiceId>,
    FillSizeWithTx<MB16, Tx>,
    FillSizeWithBlobs<MB16, BlobInfo>,
    RocksBackend<Wire>,
    KzgrsSamplingBackend<ChaCha20Rng>,
    SamplingAdapter,
    ChaCha20Rng,
    SamplingStorageAdapter<DaShare, Wire>,
    KzgrsDaVerifier,
    VerifierNetworkAdapter<NomosDaMembership, RuntimeServiceId>,
    VerifierStorageAdapter<DaShare, Wire>,
    SystemTimeBackend,
    HttApiAdapter<NomosDaMembership>,
    RuntimeServiceId,
>;

pub type NodeCryptarchia = Cryptarchia<
    nomos_da_sampling::network::adapters::validator::Libp2pAdapter<
        NomosDaMembership,
        RuntimeServiceId,
    >,
>;

pub type TxMempool = TxMempoolService<
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash, RuntimeServiceId>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    RuntimeServiceId,
>;

pub type DaMempool = DaMempoolService<
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId, RuntimeServiceId>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    KzgrsSamplingBackend<ChaCha20Rng>,
    nomos_da_sampling::network::adapters::validator::Libp2pAdapter<
        NomosDaMembership,
        RuntimeServiceId,
    >,
    ChaCha20Rng,
    SamplingStorageAdapter<DaShare, Wire>,
    KzgrsDaVerifier,
    VerifierNetworkAdapter<NomosDaMembership, RuntimeServiceId>,
    VerifierStorageAdapter<DaShare, Wire>,
    HttApiAdapter<NomosDaMembership>,
    RuntimeServiceId,
>;

pub type DaIndexer<SamplingAdapter> = DataIndexerService<
    // Indexer specific.
    DaShare,
    IndexerStorageAdapter<Wire, BlobInfo>,
    CryptarchiaConsensusAdapter<Tx, BlobInfo>,
    // Cryptarchia specific, should be the same as in `Cryptarchia` type above.
    cryptarchia_consensus::network::adapters::libp2p::LibP2pAdapter<Tx, BlobInfo, RuntimeServiceId>,
    cryptarchia_consensus::blend::adapters::libp2p::LibP2pAdapter<
        BlendNetworkAdapter<RuntimeServiceId>,
        Tx,
        BlobInfo,
        RuntimeServiceId,
    >,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash, RuntimeServiceId>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId, RuntimeServiceId>,
    FillSizeWithTx<MB16, Tx>,
    FillSizeWithBlobs<MB16, BlobInfo>,
    RocksBackend<Wire>,
    KzgrsSamplingBackend<ChaCha20Rng>,
    SamplingAdapter,
    ChaCha20Rng,
    SamplingStorageAdapter<DaShare, Wire>,
    KzgrsDaVerifier,
    VerifierNetworkAdapter<NomosDaMembership, RuntimeServiceId>,
    VerifierStorageAdapter<DaShare, Wire>,
    SystemTimeBackend,
    HttApiAdapter<NomosDaMembership>,
    RuntimeServiceId,
>;

pub type NodeDaIndexer = DaIndexer<
    nomos_da_sampling::network::adapters::validator::Libp2pAdapter<
        NomosDaMembership,
        RuntimeServiceId,
    >,
>;

pub type DaSampling<SamplingAdapter> = DaSamplingService<
    KzgrsSamplingBackend<ChaCha20Rng>,
    SamplingAdapter,
    ChaCha20Rng,
    SamplingStorageAdapter<DaShare, Wire>,
    KzgrsDaVerifier,
    VerifierNetworkAdapter<NomosDaMembership, RuntimeServiceId>,
    VerifierStorageAdapter<DaShare, Wire>,
    HttApiAdapter<NomosDaMembership>,
    RuntimeServiceId,
>;

pub type NodeDaSampling = DaSampling<SamplingLibp2pAdapter<NomosDaMembership, RuntimeServiceId>>;

pub type DaVerifier<VerifierAdapter> = DaVerifierService<
    KzgrsDaVerifier,
    VerifierAdapter,
    VerifierStorageAdapter<DaShare, Wire>,
    RuntimeServiceId,
>;

pub type NodeDaVerifier = DaVerifier<VerifierNetworkAdapter<FillFromNodeList, RuntimeServiceId>>;

pub type NomosTimeService = TimeService<SystemTimeBackend, RuntimeServiceId>;

#[derive_services]
pub struct Nomos {
    #[cfg(feature = "tracing")]
    tracing: Tracing<RuntimeServiceId>,
    network: NetworkService<NetworkBackend, RuntimeServiceId>,
    blend: BlendService<BlendBackend, BlendNetworkAdapter<RuntimeServiceId>, RuntimeServiceId>,
    da_indexer: NodeDaIndexer,
    da_verifier: NodeDaVerifier,
    da_sampling: NodeDaSampling,
    da_network: DaNetworkService<DaNetworkValidatorBackend<NomosDaMembership>, RuntimeServiceId>,
    cl_mempool: TxMempool,
    da_mempool: DaMempool,
    cryptarchia: NodeCryptarchia,
    time: NomosTimeService,
    http: NomosApiService,
    storage: StorageService<RocksBackend<Wire>, RuntimeServiceId>,
    system_sig: SystemSig<RuntimeServiceId>,
}

pub struct Wire;

impl StorageSerde for Wire {
    type Error = wire::Error;

    fn serialize<T: Serialize>(value: T) -> Bytes {
        wire::serialize(&value).unwrap().into()
    }

    fn deserialize<T: DeserializeOwned>(buff: Bytes) -> Result<T, Self::Error> {
        wire::deserialize(&buff)
    }
}
