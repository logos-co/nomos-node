mod tx;

use color_eyre::eyre::Result;
use consensus_engine::overlay::{FlatOverlay, RandomBeaconState, RoundRobin};
#[cfg(feature = "metrics")]
use metrics::{backend::map::MapMetricsBackend, types::MetricsData, MetricsService};
#[cfg(feature = "libp2p")]
use nomos_consensus::network::adapters::libp2p::Libp2pAdapter as ConsensusLibp2pAdapter;
#[cfg(feature = "waku")]
use nomos_consensus::network::adapters::waku::WakuAdapter as ConsensusWakuAdapter;
use nomos_consensus::CarnotConsensus;
use nomos_core::fountain::mock::MockFountain;
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
use overwatch_rs::services::{handle::ServiceHandle, ServiceData};
use serde::{Deserialize, Serialize};

pub use tx::Tx;

#[derive(Deserialize, Debug, Clone, Serialize)]
pub struct Config {
    pub log: <Logger as ServiceData>::Settings,
    #[cfg(feature = "waku")]
    pub network: <NetworkService<Waku> as ServiceData>::Settings,
    #[cfg(feature = "libp2p")]
    pub network: <NetworkService<Libp2p> as ServiceData>::Settings,
    pub http: <HttpService<AxumBackend> as ServiceData>::Settings,
    pub consensus: <Carnot as ServiceData>::Settings,
    #[cfg(feature = "metrics")]
    pub metrics: <MetricsService<MapMetricsBackend<MetricsData>> as ServiceData>::Settings,
}

#[cfg(feature = "waku")]
pub type Carnot = CarnotConsensus<
    ConsensusWakuAdapter,
    MockPool<Tx>,
    MempoolWakuAdapter<Tx>,
    MockFountain,
    FlatOverlay<RoundRobin, RandomBeaconState>,
>;

#[cfg(feature = "libp2p")]
pub type Carnot = CarnotConsensus<
    ConsensusLibp2pAdapter,
    MockPool<Tx>,
    MempoolLibp2pAdapter<Tx>,
    MockFountain,
    FlatOverlay<RoundRobin>,
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
}
