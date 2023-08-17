use std::net::SocketAddr;

use mixnet_topology::MixnetTopology;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixnetClientConfig {
    pub listen_addr: SocketAddr,
    pub topology: MixnetTopology,
}
