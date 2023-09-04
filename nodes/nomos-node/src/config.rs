use color_eyre::eyre::Result;
#[cfg(feature = "metrics")]
use metrics::{backend::map::MapMetricsBackend, types::MetricsData, MetricsService};
use nomos_http::{backends::axum::AxumBackend, http::HttpService};
#[cfg(feature = "libp2p")]
use nomos_libp2p::secp256k1::SecretKey;
use nomos_log::Logger;
#[cfg(feature = "libp2p")]
use nomos_network::backends::libp2p::Libp2p;
#[cfg(feature = "waku")]
use nomos_network::backends::waku::Waku;
use nomos_network::NetworkService;
use overwatch_rs::services::ServiceData;
use serde::{Deserialize, Serialize};
#[cfg(feature = "waku")]
use waku_bindings::SecretKey;

use crate::Carnot;

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
