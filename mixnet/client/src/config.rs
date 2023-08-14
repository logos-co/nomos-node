use std::net::SocketAddr;

use mixnet_topology::Topology;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub listen_addr: SocketAddr,
    pub topology: Topology,
}
