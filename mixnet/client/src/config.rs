use std::net::SocketAddr;

use mixnet_topology::MixnetTopology;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixnetClientConfig {
    pub mode: MixnetClientMode,
    pub topology: MixnetTopology,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum MixnetClientMode {
    Sender,
    SenderReceiver(SocketAddr),
}
