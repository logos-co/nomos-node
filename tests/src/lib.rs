mod nodes;
use mixnet_node::MixnetNodeConfig;
use mixnet_topology::MixnetTopology;
pub use nodes::MixNode;
pub use nodes::NomosNode;
use once_cell::sync::Lazy;

// std
use std::net::TcpListener;
use std::time::Duration;
use std::{fmt::Debug, sync::Mutex};

//crates
use fraction::Fraction;
use rand::{thread_rng, Rng};

pub fn get_available_port() -> u16 {
    let mut port = NET_PORT.lock().unwrap();
    *port += 1;
    while TcpListener::bind(("127.0.0.1", *port)).is_err() {
        *port += 1;
    }
    *port
}

#[async_trait::async_trait]
pub trait Node: Sized {
    async fn spawn_nodes(config: SpawnConfig) -> Vec<Self>;
    async fn consensus_info(&self) -> Self::ConsensusInfo;
    fn stop(&mut self);
}

#[derive(Clone)]
pub enum SpawnConfig {
    Star {
        n_participants: usize,
        threshold: Fraction,
        timeout: Duration,
        mixnet_node_configs: Vec<MixnetNodeConfig>,
        mixnet_topology: MixnetTopology,
    },
}
