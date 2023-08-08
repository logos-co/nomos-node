// std
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
// internal
use crate::{get_available_port, Node, SpawnConfig, RNG};
use consensus_engine::overlay::{FlatOverlaySettings, RoundRobin};
use consensus_engine::NodeId;
use mixnet::config::MixNode;
use nomos_consensus::{CarnotInfo, CarnotSettings};
use nomos_http::backends::axum::AxumBackendSettings;
#[cfg(feature = "libp2p")]
use nomos_libp2p::{Multiaddr, SwarmConfig};
use nomos_log::{LoggerBackend, LoggerFormat};
#[cfg(feature = "libp2p")]
use nomos_network::backends::libp2p::Libp2pInfo;
#[cfg(feature = "waku")]
use nomos_network::backends::waku::{WakuConfig, WakuInfo};
use nomos_network::NetworkConfig;
use nomos_node::Config;
#[cfg(feature = "waku")]
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
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
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
        while self.get(CARNOT_INFO_API).await.is_err() {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    #[cfg(feature = "waku")]
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

    #[cfg(feature = "waku")]
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

    #[cfg(feature = "libp2p")]
    pub async fn get_listening_address(&self) -> Multiaddr {
        self.get(NETWORK_INFO_API)
            .await
            .unwrap()
            .json::<Libp2pInfo>()
            .await
            .unwrap()
            .listen_addresses
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
                    .map(|id| {
                        create_node_config(
                            ids.iter().copied().map(NodeId::new).collect(),
                            *id,
                            threshold,
                            timeout,
                        )
                    })
                    .collect::<Vec<_>>();

                // Populate mixnet topology into configs
                let mixnodes: Vec<MixNode> = configs
                    .iter()
                    .map(|c| {
                        MixNode::new(
                            c.network.backend.mixnet.private_key,
                            c.network.backend.mixnet.external_address,
                        )
                    })
                    .collect();
                for conf in configs.iter_mut() {
                    let mut mixnodes = mixnodes.clone();
                    mixnodes.retain(|mixnode| {
                        mixnode.addr != conf.network.backend.mixnet.external_address
                    });
                    conf.network.backend.mixnet.topology = mixnodes.into();
                }

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
    nodes: Vec<NodeId>,
    private_key: [u8; 32],
    threshold: Fraction,
    timeout: Duration,
) -> Config {
    let mixnet_port = get_available_port();

    let mut config = Config {
        network: NetworkConfig {
            #[cfg(feature = "waku")]
            backend: WakuConfig {
                initial_peers: vec![],
                inner: Default::default(),
                mixnet: mixnet::config::Config {
                    listen_address: SocketAddr::new(
                        IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
                        mixnet_port,
                    ),
                    external_address: SocketAddr::new(
                        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                        mixnet_port,
                    ),
                    // TODO: use a different private key for NodeId <> MixNode unlinkability
                    private_key,
                    topology: Default::default(),
                    num_hops: 3,
                },
            },
            #[cfg(feature = "libp2p")]
            backend: SwarmConfig {
                initial_peers: vec![],
                ..Default::default()
            },
        },
        consensus: CarnotSettings {
            private_key,
            fountain_settings: (),
            overlay_settings: FlatOverlaySettings {
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
    #[cfg(feature = "waku")]
    {
        config.network.backend.inner.port = Some(get_available_port() as usize);
    }
    #[cfg(feature = "libp2p")]
    {
        config.network.backend.port = get_available_port();
    }

    config
}
