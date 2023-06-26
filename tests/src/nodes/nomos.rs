// std
use std::io::Read;
use std::net::SocketAddr;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
// internal
use crate::{get_available_port, Node, SpawnConfig, RNG};
use consensus_engine::overlay::{RoundRobin, Settings};
use nomos_consensus::{CarnotInfo, CarnotSettings};
use nomos_http::backends::axum::AxumBackendSettings;
use nomos_network::{
    backends::waku::{WakuConfig, WakuInfo},
    NetworkConfig,
};
use nomos_node::Config;
use waku_bindings::{Multiaddr, PeerId};
// crates
use once_cell::sync::Lazy;
use rand::Rng;
use reqwest::Client;
use tempfile::NamedTempFile;

static CLIENT: Lazy<Client> = Lazy::new(Client::new);
const NOMOS_BIN: &str = "../target/debug/nomos-node";
const CARNOT_INFO_API: &str = "carnot/info";
const NETWORK_INFO_API: &str = "network/info";

pub struct NomosNode {
    addr: SocketAddr,
    _tempdir: tempfile::TempDir,
    child: Child,
}

impl Drop for NomosNode {
    fn drop(&mut self) {
        let mut output = String::new();
        if let Some(stdout) = &mut self.child.stdout {
            stdout.read_to_string(&mut output).unwrap();
        }
        // self.child.stdout.as_mut().unwrap().read_to_string(&mut output).unwrap();
        println!("{} stdout: {}", self.addr, output);
        self.child.kill().unwrap();
    }
}

impl NomosNode {
    pub async fn spawn(config: &Config) -> Self {
        // Waku stores the messages in a db file in the current dir, we need a different
        // directory for each node to avoid conflicts
        let dir = tempfile::tempdir().unwrap();
        let mut file = NamedTempFile::new().unwrap();
        let config_path = file.path().to_owned();
        serde_json::to_writer(&mut file, config).unwrap();
        let child = Command::new(std::env::current_dir().unwrap().join(NOMOS_BIN))
            .arg(&config_path)
            .current_dir(dir.path())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let node = Self {
            addr: config.http.backend.address,
            child,
            _tempdir: dir,
        };
        node.wait_online().await;
        node
    }

    async fn get(&self, path: &str) -> reqwest::Result<reqwest::Response> {
        CLIENT
            .get(format!("http://{}/{}", self.addr, path))
            .send()
            .await
    }

    async fn wait_online(&self) {
        while self.get(CARNOT_INFO_API).await.is_err() {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn peer_id(&self) -> PeerId {
        self.get(NETWORK_INFO_API)
            .await
            .unwrap()
            .json::<WakuInfo>()
            .await
            .unwrap()
            .peer_id
            .unwrap()
    }

    pub async fn get_listening_address(&self) -> Multiaddr {
        self.get(NETWORK_INFO_API)
            .await
            .unwrap()
            .json::<WakuInfo>()
            .await
            .unwrap()
            .listen_addresses
            .unwrap()
            .swap_remove(0)
    }
}

#[async_trait::async_trait]
impl Node for NomosNode {
    type ConsensusInfo = CarnotInfo;

    async fn spawn_nodes(config: SpawnConfig) -> Vec<Self> {
        match config {
            SpawnConfig::Star { n_participants } => {
                let mut ids = vec![[0; 32]; n_participants];
                for id in &mut ids {
                    RNG.lock().unwrap().fill(id);
                }
                let mut configs = ids
                    .iter()
                    .map(|id| create_node_config(ids.clone(), *id))
                    .collect::<Vec<_>>();
                let mut nodes = vec![Self::spawn(&configs[0]).await];
                let listening_addr = nodes[0].get_listening_address().await;
                configs.drain(0..1);
                for conf in &mut configs {
                    conf.network
                        .backend
                        .initial_peers
                        .push(listening_addr.clone());
                    nodes.push(Self::spawn(conf).await);
                }
                nodes
            }
        }
    }

    async fn consensus_info(&self) -> Self::ConsensusInfo {
        self.get(CARNOT_INFO_API)
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    fn stop(&mut self) {
        self.child.kill().unwrap();
    }
}

fn create_node_config(nodes: Vec<[u8; 32]>, private_key: [u8; 32]) -> Config {
    let mut config = Config {
        network: NetworkConfig {
            backend: WakuConfig {
                initial_peers: vec![],
                inner: Default::default(),
            },
        },
        consensus: CarnotSettings {
            private_key,
            fountain_settings: (),
            overlay_settings: Settings {
                nodes,
                leader: RoundRobin::new(),
            },
        },
        log: Default::default(),
        http: nomos_http::http::HttpServiceSettings {
            backend: AxumBackendSettings {
                address: format!("127.0.0.1:{}", get_available_port())
                    .parse()
                    .unwrap(),
                cors_origins: vec![],
            },
        },
        #[cfg(feature = "metrics")]
        metrics: Default::default(),
    };
    config.network.backend.inner.port = Some(get_available_port() as usize);
    config
}
