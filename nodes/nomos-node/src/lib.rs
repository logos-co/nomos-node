pub mod api;
mod config;
mod tx;

use carnot_consensus::network::adapters::p2p::P2pAdapter as ConsensusNetworkAdapter;
use carnot_engine::overlay::{RandomBeaconState, RoundRobin, TreeOverlay};
use color_eyre::eyre::Result;
use full_replication::Certificate;
use full_replication::{AbsoluteNumber, Attestation, Blob, FullReplication};
#[cfg(feature = "metrics")]
use metrics::{backend::map::MapMetricsBackend, types::MetricsData, MetricsService};

use api::AxumBackend;
use bytes::Bytes;
use carnot_consensus::CarnotConsensus;
use nomos_api::ApiService;
use nomos_core::{
    da::{blob, certificate},
    header::HeaderId,
    tx::Transaction,
    wire,
};
use nomos_da::{
    backend::memory_cache::BlobCache, network::adapters::p2p::P2pAdapter as DaNetworkAdapter,
    DataAvailabilityService,
};
use nomos_log::Logger;
use nomos_mempool::network::adapters::p2p::P2pAdapter as MempoolNetworkAdapter;
use nomos_mempool::{
    backend::mockpool::MockPool, Certificate as CertDiscriminant, MempoolService,
    Transaction as TxDiscriminant,
};
#[cfg(feature = "metrics")]
use nomos_metrics::Metrics;
use nomos_network::backends::libp2p::Libp2p;
use nomos_storage::{
    backends::{sled::SledBackend, StorageSerde},
    StorageService,
};

pub use config::{
    Config, ConsensusArgs, DaArgs, HttpArgs, LogArgs, MetricsArgs, NetworkArgs, OverlayArgs,
};
use nomos_core::{
    da::certificate::select::FillSize as FillSizeWithBlobsCertificate,
    tx::select::FillSize as FillSizeWithTx,
};
use nomos_network::NetworkService;
use nomos_system_sig::SystemSig;
use overwatch_derive::*;
use overwatch_rs::services::handle::ServiceHandle;
use serde::{de::DeserializeOwned, Serialize};

pub use tx::Tx;

pub const CL_TOPIC: &str = "cl";
pub const DA_TOPIC: &str = "da";
const MB16: usize = 1024 * 1024 * 16;

pub type Carnot = CarnotConsensus<
    ConsensusNetworkAdapter,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<
        HeaderId,
        Certificate,
        <<Certificate as certificate::Certificate>::Blob as blob::Blob>::Hash,
    >,
    MempoolNetworkAdapter<
        Certificate,
        <<Certificate as certificate::Certificate>::Blob as blob::Blob>::Hash,
    >,
    TreeOverlay<RoundRobin, RandomBeaconState>,
    FillSizeWithTx<MB16, Tx>,
    FillSizeWithBlobsCertificate<MB16, Certificate>,
    SledBackend<Wire>,
>;

pub type DataAvailability = DataAvailabilityService<
    FullReplication<AbsoluteNumber<Attestation, Certificate>>,
    BlobCache<<Blob as nomos_core::da::blob::Blob>::Hash, Blob>,
    DaNetworkAdapter<Blob, Attestation>,
>;

type Mempool<K, V, D> = MempoolService<MempoolNetworkAdapter<K, V>, MockPool<HeaderId, K, V>, D>;

#[derive(Services)]
pub struct Nomos {
    logging: ServiceHandle<Logger>,
    network: ServiceHandle<NetworkService<Libp2p>>,
    cl_mempool: ServiceHandle<Mempool<Tx, <Tx as Transaction>::Hash, TxDiscriminant>>,
    da_mempool: ServiceHandle<
        Mempool<
            Certificate,
            <<Certificate as certificate::Certificate>::Blob as blob::Blob>::Hash,
            CertDiscriminant,
        >,
    >,
    consensus: ServiceHandle<Carnot>,
    http: ServiceHandle<ApiService<AxumBackend<Tx, Wire, MB16>>>,
    da: ServiceHandle<DataAvailability>,
    storage: ServiceHandle<StorageService<SledBackend<Wire>>>,
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
