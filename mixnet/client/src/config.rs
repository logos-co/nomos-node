use std::net::SocketAddr;

use mixnet_topology::MixnetTopology;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixnetClientConfig {
    pub topology: MixnetTopology,
    // A listen address for receiving final payloads from a MixnetNode.
    // If you want to run MixnetClient only with sender-mode, set [`None`].
    pub listen_address: Option<SocketAddr>,
}
