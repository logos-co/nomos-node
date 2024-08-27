pub mod api;
mod config;
mod tx;

// std
// crates
use api::AxumBackend;
use bytes::Bytes;
use color_eyre::eyre::Result;
pub use config::{Config, CryptarchiaArgs, HttpArgs, LogArgs, MetricsArgs, NetworkArgs};
use kzgrs_backend::common::blob::DaBlob;
use kzgrs_backend::dispersal::BlobInfo;
use nomos_api::ApiService;
use nomos_core::da::blob::info::DispersedBlobInfo;
pub use nomos_core::{
    da::blob::select::FillSize as FillSizeWithBlobs, tx::select::FillSize as FillSizeWithTx,
};
use nomos_core::{header::HeaderId, tx::Transaction, wire};
use nomos_da_network_service::backends::libp2p::validator::DaNetworkValidatorBackend;
use nomos_da_network_service::NetworkService as DaNetworkService;
use nomos_da_verifier::backend::kzgrs::KzgrsDaVerifier;
#[cfg(feature = "tracing")]
use nomos_log::Logger;
use nomos_mempool::da::service::DaMempoolService;
use nomos_mempool::network::adapters::libp2p::Libp2pAdapter as MempoolNetworkAdapter;
use nomos_mempool::{backend::mockpool::MockPool, TxMempoolService};
#[cfg(feature = "metrics")]
use nomos_metrics::Metrics;
use nomos_network::backends::libp2p::Libp2p as NetworkBackend;
use nomos_network::NetworkService;
use nomos_storage::{
    backends::{rocksdb::RocksBackend, StorageSerde},
    StorageService,
};
use nomos_system_sig::SystemSig;
use overwatch_derive::*;
use overwatch_rs::services::handle::ServiceHandle;
use serde::{de::DeserializeOwned, Serialize};
use subnetworks_assignations::versions::v1::FillFromNodeList;
// internal
pub use tx::Tx;

pub type NomosApiService =
    ApiService<AxumBackend<(), DaBlob, BlobInfo, BlobInfo, KzgrsDaVerifier, Tx, Wire, MB16>>;

pub const CL_TOPIC: &str = "cl";
pub const DA_TOPIC: &str = "da";
const MB16: usize = 1024 * 1024 * 16;

pub type Cryptarchia = cryptarchia_consensus::CryptarchiaConsensus<
    cryptarchia_consensus::network::adapters::libp2p::LibP2pAdapter<Tx, BlobInfo>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    FillSizeWithTx<MB16, Tx>,
    FillSizeWithBlobs<MB16, BlobInfo>,
    RocksBackend<Wire>,
>;

pub type TxMempool = TxMempoolService<
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
>;

pub type DaMempool = DaMempoolService<
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
>;

#[derive(Services)]
pub struct Nomos {
    #[cfg(feature = "tracing")]
    logging: ServiceHandle<Logger>,
    network: ServiceHandle<NetworkService<NetworkBackend>>,
    da_network: ServiceHandle<DaNetworkService<DaNetworkValidatorBackend<FillFromNodeList>>>,
    cl_mempool: ServiceHandle<TxMempool>,
    da_mempool: ServiceHandle<DaMempool>,
    cryptarchia: ServiceHandle<Cryptarchia>,
    http: ServiceHandle<NomosApiService>,
    storage: ServiceHandle<StorageService<RocksBackend<Wire>>>,
    #[cfg(feature = "metrics")]
    metrics: ServiceHandle<Metrics>,
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
