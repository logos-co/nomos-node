pub mod nodes;
use mixnet_node::MixnetNodeConfig;
use mixnet_topology::MixnetTopology;
pub use nodes::MixNode;
pub use nodes::NomosNode;
use once_cell::sync::Lazy;

use std::env;
// std
use std::net::TcpListener;
use std::ops::Mul;
use std::time::Duration;
use std::{fmt::Debug, sync::Mutex};

//crates
use fraction::{Fraction, One};
use rand::{thread_rng, Rng};

static NET_PORT: Lazy<Mutex<u16>> = Lazy::new(|| Mutex::new(thread_rng().gen_range(8000, 10000)));
static IS_SLOW_TEST_ENV: Lazy<bool> =
    Lazy::new(|| env::var("SLOW_TEST_ENV").is_ok_and(|s| s == "true"));

pub fn get_available_port() -> u16 {
    let mut port = NET_PORT.lock().unwrap();
    *port += 1;
    while TcpListener::bind(("127.0.0.1", *port)).is_err() {
        *port += 1;
    }
    *port
}

/// In slow test environments like Codecov, use 2x timeout.
pub fn adjust_timeout(d: Duration) -> Duration {
    if *IS_SLOW_TEST_ENV {
        d.mul(2)
    } else {
        d
    }
}

#[async_trait::async_trait]
pub trait Node: Sized {
    type ConsensusInfo: Debug + Clone + PartialEq;
    async fn spawn_nodes(config: SpawnConfig) -> Vec<Self>;
    async fn consensus_info(&self) -> Self::ConsensusInfo;
    fn stop(&mut self);
}

#[derive(Clone)]
pub enum SpawnConfig {
    // Star topology: Every node is initially connected to a single node.
    Star {
        consensus: ConsensusConfig,
        mixnet: MixnetConfig,
    },
    // Chain topology: Every node is chained to the node next to it.
    Chain {
        consensus: ConsensusConfig,
        mixnet: MixnetConfig,
    },
}

impl SpawnConfig {
    // Returns a SpawnConfig::Chain with proper configurations for happy-path tests
    pub fn chain_happy(n_participants: usize, mixnet_config: MixnetConfig) -> Self {
        Self::Chain {
            consensus: ConsensusConfig {
                n_participants,
                // All nodes are expected to be responsive in happy-path tests.
                threshold: Fraction::one(),
                // Set the timeout conservatively
                // since nodes should be spawned sequentially in the chain topology
                // and it takes 1+ secs for each nomos-node to be started.
                timeout: adjust_timeout(Duration::from_millis(n_participants as u64 * 2500)),
            },
            mixnet: mixnet_config,
        }
    }
}

#[derive(Clone)]
pub struct ConsensusConfig {
    pub n_participants: usize,
    pub threshold: Fraction,
    pub timeout: Duration,
}

#[derive(Clone)]
pub struct MixnetConfig {
    pub node_configs: Vec<MixnetNodeConfig>,
    pub topology: MixnetTopology,
}
