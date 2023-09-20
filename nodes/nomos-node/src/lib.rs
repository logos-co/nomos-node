mod config;
mod tx;

use color_eyre::eyre::Result;
use consensus_engine::overlay::{FlatOverlay, RandomBeaconState, RoundRobin};
use full_replication::Blob;
#[cfg(feature = "libp2p")]
use full_replication::{AbsoluteNumber, Attestation, Certificate, FullReplication};
#[cfg(feature = "metrics")]
use metrics::{backend::map::MapMetricsBackend, types::MetricsData, MetricsService};
#[cfg(feature = "libp2p")]
use nomos_consensus::network::adapters::libp2p::Libp2pAdapter as ConsensusLibp2pAdapter;
#[cfg(feature = "waku")]
use nomos_consensus::network::adapters::waku::WakuAdapter as ConsensusWakuAdapter;
use nomos_consensus::CarnotConsensus;
#[cfg(feature = "libp2p")]
use nomos_da::{
    backend::memory_cache::BlobCache, network::adapters::libp2p::Libp2pAdapter as DaLibp2pAdapter,
    DataAvailabilityService,
};
use nomos_http::backends::axum::AxumBackend;
use nomos_http::bridge::HttpBridgeService;
use nomos_http::http::HttpService;
use nomos_log::Logger;
#[cfg(feature = "libp2p")]
use nomos_mempool::network::adapters::libp2p::Libp2pAdapter as MempoolLibp2pAdapter;
#[cfg(feature = "waku")]
use nomos_mempool::network::adapters::waku::WakuAdapter as MempoolWakuAdapter;
use nomos_mempool::{backend::mockpool::MockPool, MempoolService};
#[cfg(feature = "libp2p")]
use nomos_network::backends::libp2p::Libp2p;
#[cfg(feature = "waku")]
use nomos_network::backends::waku::Waku;
use nomos_network::NetworkService;
use overwatch_derive::*;
use overwatch_rs::services::handle::ServiceHandle;

pub use config::{Config, ConsensusArgs, HttpArgs, LogArgs, NetworkArgs, OverlayArgs};
use nomos_core::{
    da::blob::select::FillSize as FillSizeWithBlobs, tx::select::FillSize as FillSizeWithTx,
};
pub use tx::Tx;

#[cfg(all(feature = "waku", feature = "libp2p"))]
compile_error!("feature \"waku\" and feature \"libp2p\" cannot be enabled at the same time");

#[cfg(feature = "waku")]
pub type Carnot = CarnotConsensus<
    ConsensusWakuAdapter,
    MockPool<Tx>,
    MempoolWakuAdapter<Tx>,
    FlatOverlay<RoundRobin, RandomBeaconState>,
    Blob,
    FillSizeWithTx<MB16, Tx>,
    FillSizeWithBlobs<MB16, Blob>,
>;

const MB16: usize = 1024 * 1024 * 16;

#[cfg(feature = "libp2p")]
pub type Carnot = CarnotConsensus<
    ConsensusLibp2pAdapter,
    MockPool<Tx>,
    MempoolLibp2pAdapter<Tx>,
    FlatOverlay<RoundRobin, RandomBeaconState>,
    Blob,
    FillSizeWithTx<MB16, Tx>,
    FillSizeWithBlobs<MB16, Blob>,
>;

#[cfg(feature = "libp2p")]
type DataAvailability = DataAvailabilityService<
    FullReplication<AbsoluteNumber<Attestation, Certificate>>,
    BlobCache<<Blob as nomos_core::da::blob::Blob>::Hash, Blob>,
    DaLibp2pAdapter<Blob, Attestation>,
>;

#[derive(Services)]
pub struct Nomos {
    logging: ServiceHandle<Logger>,
    #[cfg(feature = "waku")]
    network: ServiceHandle<NetworkService<Waku>>,
    #[cfg(feature = "libp2p")]
    network: ServiceHandle<NetworkService<Libp2p>>,
    #[cfg(feature = "waku")]
    mockpool: ServiceHandle<MempoolService<MempoolWakuAdapter<Tx>, MockPool<Tx>>>,
    #[cfg(feature = "libp2p")]
    mockpool: ServiceHandle<MempoolService<MempoolLibp2pAdapter<Tx>, MockPool<Tx>>>,
    consensus: ServiceHandle<Carnot>,
    http: ServiceHandle<HttpService<AxumBackend>>,
    bridges: ServiceHandle<HttpBridgeService>,
    #[cfg(feature = "metrics")]
    metrics: ServiceHandle<MetricsService<MapMetricsBackend<MetricsData>>>,
    #[cfg(feature = "libp2p")]
    da: ServiceHandle<DataAvailability>,
}
