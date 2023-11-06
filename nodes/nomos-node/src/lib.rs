mod config;
mod tx;

use color_eyre::eyre::Result;
use consensus_engine::overlay::{RandomBeaconState, RoundRobin, TreeOverlay};
use full_replication::Certificate;
use full_replication::{AbsoluteNumber, Attestation, Blob, FullReplication};
#[cfg(feature = "metrics")]
use metrics::{backend::map::MapMetricsBackend, types::MetricsData, MetricsService};
use nomos_consensus::network::adapters::libp2p::Libp2pAdapter as ConsensusLibp2pAdapter;

use bytes::Bytes;
use nomos_consensus::CarnotConsensus;
use nomos_core::{
    da::{blob, certificate},
    tx::Transaction,
    wire,
};
use nomos_da::{
    backend::memory_cache::BlobCache, network::adapters::libp2p::Libp2pAdapter as DaLibp2pAdapter,
    DataAvailabilityService,
};
use nomos_http::backends::axum::AxumBackend;
use nomos_http::bridge::HttpBridgeService;
use nomos_http::http::HttpService;
use nomos_log::Logger;
use nomos_mempool::network::adapters::libp2p::Libp2pAdapter as MempoolLibp2pAdapter;
use nomos_mempool::{
    backend::mockpool::MockPool, Certificate as CertDiscriminant, MempoolService,
    Transaction as TxDiscriminant,
};
use nomos_network::backends::libp2p::Libp2p;
use nomos_storage::{
    backends::{sled::SledBackend, StorageSerde},
    StorageService,
};

pub use config::{Config, ConsensusArgs, DaArgs, HttpArgs, LogArgs, NetworkArgs, OverlayArgs};
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
    ConsensusLibp2pAdapter,
    MockPool<Tx, <Tx as Transaction>::Hash>,
    MempoolLibp2pAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<Certificate, <<Certificate as certificate::Certificate>::Blob as blob::Blob>::Hash>,
    MempoolLibp2pAdapter<
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
    DaLibp2pAdapter<Blob, Attestation>,
>;

type Mempool<K, V, D> = MempoolService<MempoolLibp2pAdapter<K, V>, MockPool<K, V>, D>;

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
    http: ServiceHandle<HttpService<AxumBackend>>,
    bridges: ServiceHandle<HttpBridgeService>,
    #[cfg(feature = "metrics")]
    metrics: ServiceHandle<MetricsService<MapMetricsBackend<MetricsData>>>,
    da: ServiceHandle<DataAvailability>,
    storage: ServiceHandle<StorageService<SledBackend<Wire>>>,
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
