pub mod nodes;
// pub use nodes::NomosNode;
use once_cell::sync::Lazy;

use std::env;
// std
use std::net::TcpListener;
use std::ops::Mul;
use std::time::Duration;
use std::{fmt::Debug, sync::Mutex};

//crates
use nomos_libp2p::{Multiaddr, Swarm};
use nomos_node::Config;
use rand::{thread_rng, Rng};

static NET_PORT: Lazy<Mutex<u16>> = Lazy::new(|| Mutex::new(thread_rng().gen_range(8000..10000)));
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
    async fn spawn(mut config: Config) -> Self;
    async fn spawn_nodes(config: SpawnConfig) -> Vec<Self> {
        let mut nodes = Vec::new();
        for conf in Self::node_configs(config) {
            nodes.push(Self::spawn(conf).await);
        }
        nodes
    }
    fn node_configs(config: SpawnConfig) -> Vec<Config> {
        match config {
            SpawnConfig::Star { consensus } => {
                let mut configs = Self::create_node_configs(consensus);
                let next_leader_config = configs.remove(0);
                let first_node_addr = node_address(&next_leader_config);
                let mut node_configs = vec![next_leader_config];
                for mut conf in configs {
                    conf.network
                        .backend
                        .initial_peers
                        .push(first_node_addr.clone());

                    node_configs.push(conf);
                }
                node_configs
            }
            SpawnConfig::Chain { consensus } => {
                let mut configs = Self::create_node_configs(consensus);
                let next_leader_config = configs.remove(0);
                let mut prev_node_addr = node_address(&next_leader_config);
                let mut node_configs = vec![next_leader_config];
                for mut conf in configs {
                    conf.network.backend.initial_peers.push(prev_node_addr);
                    prev_node_addr = node_address(&conf);

                    node_configs.push(conf);
                }
                node_configs
            }
        }
    }
    fn create_node_configs(consensus: ConsensusConfig) -> Vec<Config>;
    async fn consensus_info(&self) -> Self::ConsensusInfo;
    fn stop(&mut self);
}

#[derive(Clone)]
pub enum SpawnConfig {
    // Star topology: Every node is initially connected to a single node.
    Star { consensus: ConsensusConfig },
    // Chain topology: Every node is chained to the node next to it.
    Chain { consensus: ConsensusConfig },
}

impl SpawnConfig {
    // Returns a SpawnConfig::Chain with proper configurations for happy-path tests
    pub fn chain_happy(n_participants: usize) -> Self {
        Self::Chain {
            consensus: ConsensusConfig {
                n_participants,
                // by setting the active slot coeff close to 1, we also increase the probability of multiple blocks (forks)
                // being produced in the same slot (epoch). Setting the security parameter to some value > 1
                // ensures nodes have some time to sync before deciding on the longest chain.
                security_param: 10,
                // a block should be produced (on average) every slot
                active_slot_coeff: 0.9,
            },
        }
    }

    pub fn star_happy(n_participants: usize) -> Self {
        Self::Star {
            consensus: ConsensusConfig {
                n_participants,
                // by setting the slot coeff to 1, we also increase the probability of multiple blocks (forks)
                // being produced in the same slot (epoch). Setting the security parameter to some value > 1
                // ensures nodes have some time to sync before deciding on the longest chain.
                security_param: 10,
                // a block should be produced (on average) every slot
                active_slot_coeff: 0.9,
            },
        }
    }
}

fn node_address(config: &Config) -> Multiaddr {
    Swarm::multiaddr(
        std::net::Ipv4Addr::new(127, 0, 0, 1),
        config.network.backend.inner.port,
    )
}

#[derive(Clone)]
pub struct ConsensusConfig {
    pub n_participants: usize,
    pub security_param: u32,
    pub active_slot_coeff: f64,
}
