use mixnet::{client::MixClientConfig, node::MixNodeConfig};
use nomos_libp2p::{Multiaddr, SwarmConfig};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Libp2pConfig {
    #[serde(flatten)]
    pub inner: SwarmConfig,
    // Initial peers to connect to
    #[serde(default)]
    pub initial_peers: Vec<Multiaddr>,
    pub mixclient_config: MixClientConfig,
    pub mixnode_config: MixNodeConfig,
}
