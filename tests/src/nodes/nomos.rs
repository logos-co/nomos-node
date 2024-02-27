// std
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
// internal
use super::{create_tempdir, persist_tempdir, LOGS_PREFIX};
use crate::{adjust_timeout, get_available_port, ConsensusConfig, MixnetConfig, Node, SpawnConfig};
use carnot_consensus::{CarnotInfo, CarnotSettings};
use carnot_engine::overlay::{RandomBeaconState, RoundRobin, TreeOverlay, TreeOverlaySettings};
use carnot_engine::{BlockId, NodeId, Overlay};
use full_replication::Certificate;
use mixnet::address::NodeAddress;
use mixnet::client::MixClientConfig;
use mixnet::crypto::{PrivateKey, PublicKey};
use mixnet::node::MixNodeConfig;
use mixnet::topology::{MixNodeInfo, MixnetTopology};
use nomos_core::block::Block;
use nomos_libp2p::{Multiaddr, Swarm};
use nomos_log::{LoggerBackend, LoggerFormat};
use nomos_mempool::MempoolMetrics;
use nomos_network::backends::libp2p::Libp2pConfig;
use nomos_network::NetworkConfig;
use nomos_node::{api::AxumBackendSettings, Config, Tx};
// crates
use fraction::Fraction;
use once_cell::sync::Lazy;
use rand::{thread_rng, Rng};
use reqwest::{Client, Url};
use tempfile::NamedTempFile;

static CLIENT: Lazy<Client> = Lazy::new(Client::new);
const NOMOS_BIN: &str = "../target/debug/nomos-node";
const CARNOT_INFO_API: &str = "carnot/info";
const STORAGE_BLOCKS_API: &str = "storage/block";
const GET_BLOCKS_INFO: &str = "carnot/blocks";

pub struct NomosNode {
    addr: SocketAddr,
    _tempdir: tempfile::TempDir,
    child: Child,
    config: Config,
}

impl Drop for NomosNode {
    fn drop(&mut self) {
        if std::thread::panicking() {
            if let Err(e) = persist_tempdir(&mut self._tempdir, "nomos-node") {
                println!("failed to persist tempdir: {e}");
            }
        }

        if let Err(e) = self.child.kill() {
            println!("failed to kill the child process: {e}");
        }
    }
}

impl NomosNode {
    pub fn id(&self) -> NodeId {
        NodeId::from(self.config.consensus.private_key)
    }

    pub async fn spawn(mut config: Config) -> Self {
        // Waku stores the messages in a db file in the current dir, we need a different
        // directory for each node to avoid conflicts
        let dir = create_tempdir().unwrap();
        let mut file = NamedTempFile::new().unwrap();
        let config_path = file.path().to_owned();

        // setup logging so that we can intercept it later in testing
        config.log.backend = LoggerBackend::File {
            directory: dir.path().to_owned(),
            prefix: Some(LOGS_PREFIX.into()),
        };
        config.log.format = LoggerFormat::Json;

        serde_yaml::to_writer(&mut file, &config).unwrap();
        let child = Command::new(std::env::current_dir().unwrap().join(NOMOS_BIN))
            .arg(&config_path)
            .current_dir(dir.path())
            .stdout(Stdio::inherit())
            .spawn()
            .unwrap();
        let node = Self {
            addr: config.http.backend_settings.address,
            child,
            _tempdir: dir,
            config,
        };
        tokio::time::timeout(adjust_timeout(Duration::from_secs(10)), async {
            node.wait_online().await
        })
        .await
        .unwrap();

        node
    }

    async fn get(&self, path: &str) -> reqwest::Result<reqwest::Response> {
        CLIENT
            .get(format!("http://{}/{}", self.addr, path))
            .send()
            .await
    }

    async fn wait_online(&self) {
        loop {
            let res = self.get(CARNOT_INFO_API).await;
            if res.is_ok() && res.unwrap().status().is_success() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub fn url(&self) -> Url {
        format!("http://{}", self.addr).parse().unwrap()
    }

    pub async fn get_block(&self, id: BlockId) -> Option<Block<Tx, Certificate>> {
        CLIENT
            .post(&format!("http://{}/{}", self.addr, STORAGE_BLOCKS_API))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&id).unwrap())
            .send()
            .await
            .unwrap()
            .json::<Option<Block<Tx, Certificate>>>()
            .await
            .unwrap()
    }

    pub async fn get_mempoool_metrics(&self, pool: Pool) -> MempoolMetrics {
        let discr = match pool {
            Pool::Cl => "cl",
            Pool::Da => "da",
        };
        let addr = format!("{}/metrics", discr);
        let res = self
            .get(&addr)
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        MempoolMetrics {
            pending_items: res["pending_items"].as_u64().unwrap() as usize,
            last_item_timestamp: res["last_item_timestamp"].as_u64().unwrap(),
        }
    }

    pub async fn get_blocks_info(
        &self,
        from: Option<BlockId>,
        to: Option<BlockId>,
    ) -> Vec<carnot_engine::Block> {
        let mut req = CLIENT.get(format!("http://{}/{}", self.addr, GET_BLOCKS_INFO));

        if let Some(from) = from {
            req = req.query(&[("from", from)]);
        }

        if let Some(to) = to {
            req = req.query(&[("to", to)]);
        }

        req.send()
            .await
            .unwrap()
            .json::<Vec<carnot_engine::Block>>()
            .await
            .unwrap()
    }

    // not async so that we can use this in `Drop`
    pub fn get_logs_from_file(&self) -> String {
        println!(
            "fetching logs from dir {}...",
            self._tempdir.path().display()
        );
        // std::thread::sleep(std::time::Duration::from_secs(50));
        std::fs::read_dir(self._tempdir.path())
            .unwrap()
            .filter_map(|entry| {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.is_file() && path.to_str().unwrap().contains(LOGS_PREFIX) {
                    Some(path)
                } else {
                    None
                }
            })
            .map(|f| std::fs::read_to_string(f).unwrap())
            .collect::<String>()
    }

    pub fn config(&self) -> &Config {
        &self.config
    }
}

#[async_trait::async_trait]
impl Node for NomosNode {
    type ConsensusInfo = CarnotInfo;

    /// Spawn nodes sequentially.
    /// After one node is spawned successfully, the next node is spawned.
    async fn spawn_nodes(config: SpawnConfig) -> Vec<Self> {
        let mut nodes = Vec::new();
        for conf in Self::node_configs(config) {
            nodes.push(Self::spawn(conf).await);
        }
        nodes
    }

    fn node_configs(config: SpawnConfig) -> Vec<nomos_node::Config> {
        match config {
            SpawnConfig::Star { consensus } => {
                let (next_leader_config, configs) = create_node_configs(consensus);

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
                let (next_leader_config, configs) = create_node_configs(consensus);

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

    async fn consensus_info(&self) -> Self::ConsensusInfo {
        let res = self.get(CARNOT_INFO_API).await;
        res.unwrap().json().await.unwrap()
    }

    fn stop(&mut self) {
        self.child.kill().unwrap();
    }
}

const NUM_MIXNODE_CANDIDATES: usize = 2;

/// Returns the config of the next leader and all other nodes.
///
/// Depending on the network topology, the next leader must be spawned first,
/// so the leader can receive votes from all other nodes that will be subsequently spawned.
/// If not, the leader will miss votes from nodes spawned before itself.
/// This issue will be resolved by devising the block catch-up mechanism in the future.
fn create_node_configs(consensus: ConsensusConfig) -> (Config, Vec<Config>) {
    let mut ids = vec![[0; 32]; consensus.n_participants];
    for id in &mut ids {
        thread_rng().fill(id);
    }
    let ports: Vec<u16> = (0..consensus.n_participants)
        .map(|_| get_available_port())
        .collect();

    let mixnet = create_mixnet_config(&ports);

    let mut configs = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            create_node_config(
                ids.iter().copied().map(NodeId::new).collect(),
                *id,
                ports[i],
                consensus.threshold,
                consensus.timeout,
                mixnet.mixclient_config.clone(),
                mixnet.mixnode_configs[i].clone(),
            )
        })
        .collect::<Vec<_>>();

    let overlay = TreeOverlay::new(configs[0].consensus.overlay_settings.clone());
    let next_leader = overlay.next_leader();
    let next_leader_idx = ids
        .iter()
        .position(|&id| NodeId::from(id) == next_leader)
        .unwrap();

    let mut next_leader_config = configs.swap_remove(next_leader_idx);

    // Build a topology using only a subset of nodes.
    let mut mixnode_candidates = vec![&next_leader_config];
    configs
        .iter()
        .take(NUM_MIXNODE_CANDIDATES - 1)
        .for_each(|config| mixnode_candidates.push(config));
    let topology = build_mixnet_topology(&mixnode_candidates);

    // Set the topology to all configs
    next_leader_config.network.backend.mixclient_config.topology = topology.clone();
    configs.iter_mut().for_each(|config| {
        config.network.backend.mixclient_config.topology = topology.clone();
    });

    (next_leader_config, configs)
}

fn create_node_config(
    nodes: Vec<NodeId>,
    id: [u8; 32],
    port: u16,
    threshold: Fraction,
    timeout: Duration,
    mixclient_config: MixClientConfig,
    mixnode_config: MixNodeConfig,
) -> Config {
    let mut config = Config {
        network: NetworkConfig {
            backend: Libp2pConfig {
                inner: Default::default(),
                initial_peers: vec![],
                mixclient_config,
                mixnode_config,
            },
        },
        consensus: CarnotSettings {
            private_key: id,
            overlay_settings: TreeOverlaySettings {
                nodes,
                leader: RoundRobin::new(),
                current_leader: [0; 32].into(),
                number_of_committees: 1,
                committee_membership: RandomBeaconState::initial_sad_from_entropy([0; 32]),
                // By setting the threshold to 1 we ensure that all nodes come
                // online before progressing. This is only necessary until we add a way
                // to recover poast blocks from other nodes.
                super_majority_threshold: Some(threshold),
            },
            timeout,
            transaction_selector_settings: (),
            blob_selector_settings: (),
        },
        log: Default::default(),
        http: nomos_api::ApiServiceSettings {
            backend_settings: AxumBackendSettings {
                address: format!("127.0.0.1:{}", get_available_port())
                    .parse()
                    .unwrap(),
                cors_origins: vec![],
            },
        },
        da: nomos_da::Settings {
            da_protocol: full_replication::Settings {
                voter: id,
                num_attestations: 1,
            },
            backend: nomos_da::backend::memory_cache::BlobCacheSettings {
                max_capacity: usize::MAX,
                evicting_period: Duration::from_secs(60 * 60 * 24), // 1 day
            },
        },
    };

    config.network.backend.inner.port = port;
    config.log.level = tracing::Level::INFO;

    config
}

fn create_mixnet_config(ports: &[u16]) -> MixnetConfig {
    let mixnode_configs: Vec<MixNodeConfig> = ports
        .iter()
        .map(|_| MixNodeConfig {
            encryption_private_key: PrivateKey::new(),
            delay_rate_per_min: 100000000.0,
        })
        .collect();
    // Build an empty topology because it will be constructed with meaningful node infos later
    let topology = MixnetTopology::new(Vec::new(), 0, 0, [1u8; 32]).unwrap();

    MixnetConfig {
        mixclient_config: MixClientConfig {
            topology,
            emission_rate_per_min: 500.0,
            redundancy: 1,
        },
        mixnode_configs,
    }
}

fn build_mixnet_topology(mixnode_candidates: &[&Config]) -> MixnetTopology {
    let candidates = mixnode_candidates
        .iter()
        .map(|config| {
            MixNodeInfo::new(
                NodeAddress::from(SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    config.network.backend.inner.port,
                )),
                PublicKey::from(&config.network.backend.mixnode_config.encryption_private_key),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let num_layers = candidates.len();
    MixnetTopology::new(candidates, num_layers, 1, [1u8; 32]).unwrap()
}

fn node_address(config: &Config) -> Multiaddr {
    Swarm::multiaddr(
        std::net::Ipv4Addr::new(127, 0, 0, 1),
        config.network.backend.inner.port,
    )
}

pub enum Pool {
    Da,
    Cl,
}
