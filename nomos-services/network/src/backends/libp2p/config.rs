use nomos_libp2p::{Multiaddr, SwarmConfig};
use serde::{Deserialize, Serialize};

#[cfg(feature = "mixnet")]
use crate::backends::libp2p::mixnet::MixnetConfig;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Libp2pConfig {
    #[serde(flatten)]
    pub inner: SwarmConfig,
    // Initial peers to connect to
    #[serde(default)]
    pub initial_peers: Vec<Multiaddr>,
    #[cfg(feature = "mixnet")]
    pub mixnet: MixnetConfig,
}
