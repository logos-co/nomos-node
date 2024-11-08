pub mod api;
pub mod config;
mod tx;

// std
// crates
use api::backend::AxumBackend;
use bytes::Bytes;
use color_eyre::eyre::Result;
pub use config::{Config, CryptarchiaArgs, HttpArgs, LogArgs, NetworkArgs};
use kzgrs_backend::common::blob::DaBlob;
pub use kzgrs_backend::dispersal::BlobInfo;
use nomos_api::ApiService;
pub use nomos_core::da::blob::info::DispersedBlobInfo;
pub use nomos_core::{
    da::blob::select::FillSize as FillSizeWithBlobs, tx::select::FillSize as FillSizeWithTx,
};
pub use nomos_core::{header::HeaderId, tx::Transaction, wire};
use nomos_da_indexer::consensus::adapters::cryptarchia::CryptarchiaConsensusAdapter;
use nomos_da_indexer::storage::adapters::rocksdb::RocksAdapter as IndexerStorageAdapter;
use nomos_da_indexer::DataIndexerService;
pub use nomos_da_network_service::backends::libp2p::validator::DaNetworkValidatorBackend;
pub use nomos_da_network_service::NetworkService as DaNetworkService;
use nomos_da_sampling::backend::kzgrs::KzgrsSamplingBackend;
use nomos_da_sampling::network::adapters::validator::Libp2pAdapter as SamplingLibp2pAdapter;
use nomos_da_sampling::storage::adapters::rocksdb::RocksAdapter as SamplingStorageAdapter;
use nomos_da_sampling::DaSamplingService;
use nomos_da_verifier::backend::kzgrs::KzgrsDaVerifier;
use nomos_da_verifier::network::adapters::validator::Libp2pAdapter as VerifierNetworkAdapter;
use nomos_da_verifier::storage::adapters::rocksdb::RocksAdapter as VerifierStorageAdapter;
use nomos_da_verifier::DaVerifierService;
pub use nomos_mempool::da::service::{DaMempoolService, DaMempoolSettings};
pub use nomos_mempool::network::adapters::libp2p::{
    Libp2pAdapter as MempoolNetworkAdapter, Settings as MempoolAdapterSettings,
};
pub use nomos_mempool::TxMempoolSettings;
use nomos_mempool::{backend::mockpool::MockPool, TxMempoolService};
pub use nomos_mix_service::backends::libp2p::Libp2pMixBackend as MixBackend;
pub use nomos_mix_service::network::libp2p::Libp2pAdapter as MixNetworkAdapter;
pub use nomos_mix_service::MixService;
pub use nomos_network::backends::libp2p::Libp2p as NetworkBackend;
pub use nomos_network::NetworkService;
pub use nomos_storage::{
    backends::{
        rocksdb::{RocksBackend, RocksBackendSettings},
        StorageSerde,
    },
    StorageService,
};
pub use nomos_system_sig::SystemSig;
#[cfg(feature = "tracing")]
pub use nomos_tracing_service::Tracing;
use overwatch_derive::*;
use overwatch_rs::services::handle::ServiceHandle;
use rand_chacha::ChaCha20Rng;
use serde::{de::DeserializeOwned, Serialize};
use subnetworks_assignations::versions::v1::FillFromNodeList;
// internal
pub use tx::Tx;

/// Membership used by the DA Network service.
pub type NomosDaMembership = FillFromNodeList;

pub type NomosApiService = ApiService<
    AxumBackend<
        (),
        DaBlob,
        BlobInfo,
        NomosDaMembership,
        BlobInfo,
        KzgrsDaVerifier,
        Tx,
        Wire,
        KzgrsSamplingBackend<ChaCha20Rng>,
        nomos_da_sampling::network::adapters::validator::Libp2pAdapter<NomosDaMembership>,
        ChaCha20Rng,
        SamplingStorageAdapter<DaBlob, Wire>,
        MB16,
    >,
>;

pub const CONSENSUS_TOPIC: &str = "/cryptarchia/proto";
pub const CL_TOPIC: &str = "cl";
pub const DA_TOPIC: &str = "da";
pub const MB16: usize = 1024 * 1024 * 16;

pub type Cryptarchia<SamplingAdapter> = cryptarchia_consensus::CryptarchiaConsensus<
    cryptarchia_consensus::network::adapters::libp2p::LibP2pAdapter<Tx, BlobInfo>,
    cryptarchia_consensus::mix::adapters::libp2p::LibP2pAdapter<MixNetworkAdapter, Tx, BlobInfo>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    FillSizeWithTx<MB16, Tx>,
    FillSizeWithBlobs<MB16, BlobInfo>,
    RocksBackend<Wire>,
    KzgrsSamplingBackend<ChaCha20Rng>,
    SamplingAdapter,
    ChaCha20Rng,
    SamplingStorageAdapter<DaBlob, Wire>,
>;

pub type NodeCryptarchia =
    Cryptarchia<nomos_da_sampling::network::adapters::validator::Libp2pAdapter<NomosDaMembership>>;

pub type TxMempool = TxMempoolService<
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
>;

pub type DaMempool = DaMempoolService<
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    KzgrsSamplingBackend<ChaCha20Rng>,
    nomos_da_sampling::network::adapters::validator::Libp2pAdapter<NomosDaMembership>,
    ChaCha20Rng,
    SamplingStorageAdapter<DaBlob, Wire>,
>;

pub type DaIndexer<SamplingAdapter> = DataIndexerService<
    // Indexer specific.
    Bytes,
    IndexerStorageAdapter<Wire, BlobInfo>,
    CryptarchiaConsensusAdapter<Tx, BlobInfo>,
    // Cryptarchia specific, should be the same as in `Cryptarchia` type above.
    cryptarchia_consensus::network::adapters::libp2p::LibP2pAdapter<Tx, BlobInfo>,
    cryptarchia_consensus::mix::adapters::libp2p::LibP2pAdapter<MixNetworkAdapter, Tx, BlobInfo>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    FillSizeWithTx<MB16, Tx>,
    FillSizeWithBlobs<MB16, BlobInfo>,
    RocksBackend<Wire>,
    KzgrsSamplingBackend<ChaCha20Rng>,
    SamplingAdapter,
    ChaCha20Rng,
    SamplingStorageAdapter<DaBlob, Wire>,
>;

pub type NodeDaIndexer =
    DaIndexer<nomos_da_sampling::network::adapters::validator::Libp2pAdapter<NomosDaMembership>>;

pub type DaSampling<SamplingAdapter> = DaSamplingService<
    KzgrsSamplingBackend<ChaCha20Rng>,
    SamplingAdapter,
    ChaCha20Rng,
    SamplingStorageAdapter<DaBlob, Wire>,
>;

pub type NodeDaSampling = DaSampling<SamplingLibp2pAdapter<NomosDaMembership>>;

pub type DaVerifier<VerifierAdapter> =
    DaVerifierService<KzgrsDaVerifier, VerifierAdapter, VerifierStorageAdapter<(), DaBlob, Wire>>;

pub type NodeDaVerifier = DaVerifier<VerifierNetworkAdapter<FillFromNodeList>>;

#[derive(Services)]
pub struct Nomos {
    #[cfg(feature = "tracing")]
    tracing: ServiceHandle<Tracing>,
    network: ServiceHandle<NetworkService<NetworkBackend>>,
    mix: ServiceHandle<MixService<MixBackend, MixNetworkAdapter>>,
    da_indexer: ServiceHandle<NodeDaIndexer>,
    da_verifier: ServiceHandle<NodeDaVerifier>,
    da_sampling: ServiceHandle<NodeDaSampling>,
    da_network: ServiceHandle<DaNetworkService<DaNetworkValidatorBackend<NomosDaMembership>>>,
    cl_mempool: ServiceHandle<TxMempool>,
    da_mempool: ServiceHandle<DaMempool>,
    cryptarchia: ServiceHandle<NodeCryptarchia>,
    http: ServiceHandle<NomosApiService>,
    storage: ServiceHandle<StorageService<RocksBackend<Wire>>>,
    system_sig: ServiceHandle<SystemSig>,
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
