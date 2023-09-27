mod config;
mod tx;

use color_eyre::eyre::Result;
use consensus_engine::overlay::{RandomBeaconState, RoundRobin, TreeOverlay};
use full_replication::Certificate;
use full_replication::{AbsoluteNumber, Attestation, Blob, FullReplication};
#[cfg(feature = "metrics")]
use metrics::{backend::map::MapMetricsBackend, types::MetricsData, MetricsService};
use nomos_consensus::network::adapters::libp2p::Libp2pAdapter as ConsensusLibp2pAdapter;

use nomos_consensus::CarnotConsensus;
use nomos_core::tx::Transaction;
use nomos_da::{
    backend::memory_cache::BlobCache, network::adapters::libp2p::Libp2pAdapter as DaLibp2pAdapter,
    DataAvailabilityService,
};
use nomos_http::backends::axum::AxumBackend;
use nomos_http::bridge::HttpBridgeService;
use nomos_http::http::HttpService;
use nomos_log::Logger;
use nomos_mempool::network::adapters::libp2p::Libp2pAdapter as MempoolLibp2pAdapter;

use nomos_mempool::{backend::mockpool::MockPool, MempoolService};
use nomos_network::backends::libp2p::Libp2p;

use nomos_network::NetworkService;
use overwatch_derive::*;
use overwatch_rs::services::handle::ServiceHandle;

pub use config::{Config, ConsensusArgs, HttpArgs, LogArgs, NetworkArgs, OverlayArgs};
use nomos_core::{
    da::certificate::select::FillSize as FillSizeWithBlobsCertificate,
    tx::select::FillSize as FillSizeWithTx,
};
pub use tx::Tx;

const MB16: usize = 1024 * 1024 * 16;

pub type Carnot = CarnotConsensus<
    ConsensusLibp2pAdapter,
    MockPool<Tx, <Tx as Transaction>::Hash>,
    MempoolLibp2pAdapter<Tx, <Tx as Transaction>::Hash>,
    TreeOverlay<RoundRobin, RandomBeaconState>,
    Certificate,
    FillSizeWithTx<MB16, Tx>,
    FillSizeWithBlobsCertificate<MB16, Certificate>,
>;

type DataAvailability = DataAvailabilityService<
    FullReplication<AbsoluteNumber<Attestation, Certificate>>,
    BlobCache<<Blob as nomos_core::da::blob::Blob>::Hash, Blob>,
    DaLibp2pAdapter<Blob, Attestation>,
>;

type Mempool = MempoolService<
    MempoolLibp2pAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<Tx, <Tx as Transaction>::Hash>,
>;

#[derive(Services)]
pub struct Nomos {
    logging: ServiceHandle<Logger>,
    network: ServiceHandle<NetworkService<Libp2p>>,
    mockpool: ServiceHandle<Mempool>,
    consensus: ServiceHandle<Carnot>,
    http: ServiceHandle<HttpService<AxumBackend>>,
    bridges: ServiceHandle<HttpBridgeService>,
    #[cfg(feature = "metrics")]
    metrics: ServiceHandle<MetricsService<MapMetricsBackend<MetricsData>>>,
    da: ServiceHandle<DataAvailability>,
}
