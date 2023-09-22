mod config;
mod tx;

use color_eyre::eyre::Result;
use consensus_engine::overlay::{RandomBeaconState, RoundRobin, TreeOverlay};
use full_replication::Blob;

use full_replication::{AbsoluteNumber, Attestation, Certificate, FullReplication};
#[cfg(feature = "metrics")]
use metrics::{backend::map::MapMetricsBackend, types::MetricsData, MetricsService};
use nomos_consensus::network::adapters::libp2p::Libp2pAdapter as ConsensusLibp2pAdapter;

use nomos_consensus::CarnotConsensus;

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
    da::blob::select::FillSize as FillSizeWithBlobs, tx::select::FillSize as FillSizeWithTx,
};
pub use tx::Tx;

const MB16: usize = 1024 * 1024 * 16;

pub type Carnot = CarnotConsensus<
    ConsensusLibp2pAdapter,
    MockPool<Tx>,
    MempoolLibp2pAdapter<Tx>,
    TreeOverlay<RoundRobin, RandomBeaconState>,
    Blob,
    FillSizeWithTx<MB16, Tx>,
    FillSizeWithBlobs<MB16, Blob>,
>;

type DataAvailability = DataAvailabilityService<
    FullReplication<AbsoluteNumber<Attestation, Certificate>>,
    BlobCache<<Blob as nomos_core::da::blob::Blob>::Hash, Blob>,
    DaLibp2pAdapter<Blob, Attestation>,
>;

#[derive(Services)]
pub struct Nomos {
    logging: ServiceHandle<Logger>,
    network: ServiceHandle<NetworkService<Libp2p>>,
    mockpool: ServiceHandle<MempoolService<MempoolLibp2pAdapter<Tx>, MockPool<Tx>>>,
    consensus: ServiceHandle<Carnot>,
    http: ServiceHandle<HttpService<AxumBackend>>,
    bridges: ServiceHandle<HttpBridgeService>,
    #[cfg(feature = "metrics")]
    metrics: ServiceHandle<MetricsService<MapMetricsBackend<MetricsData>>>,
    da: ServiceHandle<DataAvailability>,
}
