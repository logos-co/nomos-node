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
#[cfg(feature = "libp2p")]
use nomos_libp2p::secp256k1::SecretKey;
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
#[cfg(feature = "waku")]
use waku_bindings::SecretKey;

pub use tx::Tx;

#[cfg(all(feature = "waku", feature = "libp2p"))]
compile_error!("feature \"waku\" and feature \"libp2p\" cannot be enabled at the same time");

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

impl Config {
    pub fn set_node_key(&mut self, node_key: Option<String>) -> Result<()> {
        if let Some(node_key) = node_key {
            #[cfg(feature = "waku")]
            {
                use std::str::FromStr;
                self.network.backend.inner.node_key = Some(SecretKey::from_str(&node_key)?)
            }
            #[cfg(feature = "libp2p")]
            {
                let mut key_bytes = hex::decode(node_key)?;
                self.network.backend.node_key =
                    SecretKey::try_from_bytes(key_bytes.as_mut_slice())?;
            }
        }
        Ok(())
    }
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
    FlatOverlay<RoundRobin, RandomBeaconState>,
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
