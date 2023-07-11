// std
use std::net::SocketAddr;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
// internal
use crate::{get_available_port, Node, SpawnConfig, RNG};
use consensus_engine::overlay::{RoundRobin, Settings};
use nomos_consensus::{CarnotInfo, CarnotSettings};
use nomos_http::backends::axum::AxumBackendSettings;
use nomos_log::{LoggerBackend, LoggerFormat};
use nomos_network::{
    backends::waku::{WakuConfig, WakuInfo},
    NetworkConfig,
};
use nomos_node::Config;
use waku_bindings::{Multiaddr, PeerId};
// crates
use fraction::Fraction;
use once_cell::sync::Lazy;
use rand::Rng;
use reqwest::Client;
use tempfile::NamedTempFile;

static CLIENT: Lazy<Client> = Lazy::new(Client::new);
const NOMOS_BIN: &str = "../target/debug/nomos-node";
const CARNOT_INFO_API: &str = "carnot/info";
const NETWORK_INFO_API: &str = "network/info";
const LOGS_PREFIX: &str = "__logs";

pub struct NomosNode {
    addr: SocketAddr,
    _tempdir: tempfile::TempDir,
    child: Child,
}

impl Drop for NomosNode {
    fn drop(&mut self) {
        if std::thread::panicking() {
            println!("persisting directory at {}", self._tempdir.path().display());
            // we need ownership of the dir to persist it
            let dir = std::mem::replace(&mut self._tempdir, tempfile::tempdir().unwrap());
            // a bit confusing but `into_path` persists the directory
            let _ = dir.into_path();
        }

        self.child.kill().unwrap();
    }
}

impl NomosNode {
    pub async fn spawn(mut config: Config) -> Self {
        // Waku stores the messages in a db file in the current dir, we need a different
        // directory for each node to avoid conflicts
        let dir = tempfile::tempdir().unwrap();
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
}

#[async_trait::async_trait]
impl Node for NomosNode {
    type ConsensusInfo = CarnotInfo;

    async fn spawn_nodes(config: SpawnConfig) -> Vec<Self> {
        match config {
            SpawnConfig::Star {
                n_participants,
                threshold,
                timeout,
            } => {
                let mut ids = vec![[0; 32]; n_participants];
                for id in &mut ids {
                    RNG.lock().unwrap().fill(id);
                }
                let mut configs = ids
                    .iter()
                    .map(|id| create_node_config(ids.clone(), *id, threshold, timeout))
                    .collect::<Vec<_>>();
                let mut nodes = vec![Self::spawn(configs.swap_remove(0)).await];
                let listening_addr = nodes[0].get_listening_address().await;
                for mut conf in configs {
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

fn create_node_config(
    nodes: Vec<[u8; 32]>,
    private_key: [u8; 32],
    threshold: Fraction,
    timeout: Duration,
) -> Config {
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
                // By setting the leader_threshold to 1 we ensure that all nodes come
                // online before progressing. This is only necessary until we add a way
                // to recover poast blocks from other nodes.
                leader_super_majority_threshold: Some(threshold),
            },
            timeout,
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
