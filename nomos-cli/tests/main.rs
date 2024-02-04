// use std::{env::current_dir, net::SocketAddr, path::PathBuf, thread::Builder, time::Duration};

// use carnot_consensus::CarnotSettings;
// use carnot_engine::{
//     overlay::{RandomBeaconState, RoundRobin, TreeOverlay, TreeOverlaySettings},
//     NodeId, Overlay,
// };
// use fraction::{Fraction, One};
// use mixnet_client::MixnetClientConfig;
// use mixnet_node::{MixnetNodeConfig, PRIVATE_KEY_SIZE};
// use mixnet_topology::{Layer, MixnetTopology, Node};
// use mixnode::{MixNode, MixNodeServiceSettings};
// use nomos_cli::{
//     cmds::chat::{App, ChatMessage},
//     da::disseminate::{DaProtocolChoice, FullReplicationSettings, ProtocolSettings},
// };
// use nomos_network::{
//     backends::libp2p::{Libp2p, Libp2pConfig},
//     NetworkConfig,
// };
// use nomos_node::{api::AxumBackendSettings, run, Config};
// use nomos_storage::backends::rocksdb::RocksBackendSettings;
// use overwatch_rs::{overwatch::OverwatchRunner, DynError};
// use rand::{thread_rng, RngCore};
// use reqwest::Url;
// use tracing::Level;

// fn run_mixnode(
//     name: &'static str,
//     listen_addr: SocketAddr,
//     client_listen_addr: SocketAddr,
// ) -> MixnetNodeConfig {
//     let mut private_key = [0u8; PRIVATE_KEY_SIZE];
//     thread_rng().fill_bytes(&mut private_key);
//     let config = MixnetNodeConfig {
//         listen_address: listen_addr,
//         client_listen_address: client_listen_addr,
//         private_key,
//         connection_pool_size: 255,
//         ..Default::default()
//     };
//     let config1 = config.clone();
//     Builder::new()
//         .name(name.to_string())
//         .spawn(move || {
//             let mut cfg = MixNodeServiceSettings {
//                 node: config1,
//                 logging: Default::default(),
//             };
//             cfg.logging.level = Level::WARN;

//             let app = OverwatchRunner::<MixNode>::run(cfg, None)?;
//             app.wait_finished();
//             std::result::Result::<_, DynError>::Ok(())
//         })
//         .unwrap();
//     config
// }

// fn build_topology(configs: Vec<MixnetNodeConfig>) -> MixnetTopology {
//     // Build three empty layers first
//     let mut layers = vec![Layer { nodes: Vec::new() }; 3];
//     let mut layer_id = 0;

//     // Assign nodes to each layer in round-robin
//     for config in &configs {
//         let public_key = config.public_key();
//         layers.get_mut(layer_id).unwrap().nodes.push(Node {
//             address: config.listen_address,
//             public_key,
//         });
//         layer_id = (layer_id + 1) % layers.len();
//     }

//     // Exclude empty layers
//     MixnetTopology {
//         layers: layers
//             .iter()
//             .filter(|layer| !layer.nodes.is_empty())
//             .cloned()
//             .collect(),
//     }
// }

// fn run_nomos_node(name: &'static str, mut config: Config) {
//     Builder::new()
//         .name(name.into())
//         .spawn(move || {
//             config.log.level = Level::WARN;
//             run(config)
//         })
//         .unwrap();
// }

// fn run_explorer(explorer_db_dir: PathBuf) {
//     Builder::new()
//         .name("explorer".into())
//         .spawn(|| {
//             let mut cfg = nomos_explorer::Config {
//                 log: Default::default(),
//                 api: nomos_api::ApiServiceSettings {
//                     backend_settings: nomos_explorer::AxumBackendSettings {
//                         address: "127.0.0.1:5000".parse().unwrap(),
//                         cors_origins: Vec::new(),
//                     },
//                 },
//                 storage: nomos_storage::backends::rocksdb::RocksBackendSettings {
//                     db_path: explorer_db_dir,
//                     read_only: true,
//                     column_family: Some("blocks".into()),
//                 },
//             };
//             cfg.log.level = Level::WARN;
//             nomos_explorer::Explorer::run(cfg).unwrap()
//         })
//         .unwrap();
// }

// fn run_chat(
//     username: String,
//     config: &'static str,
//     node_url: Url,
//     explorer_url: Url,
//     network: NetworkConfig<Libp2p>,
// ) -> (std::sync::mpsc::Receiver<Vec<ChatMessage>>, App) {
//     let c = nomos_cli::cmds::chat::NomosChat {
//         network_config: current_dir().unwrap().join(config),
//         da_protocol: DaProtocolChoice {
//             da_protocol: nomos_cli::da::disseminate::Protocol::FullReplication,
//             settings: ProtocolSettings {
//                 full_replication: FullReplicationSettings {
//                     voter: [0; 32],
//                     num_attestations: 1,
//                 },
//             },
//         },
//         node: node_url,
//         explorer: explorer_url,
//         message: None,
//         author: None,
//     };
//     c.run_app(username).unwrap()
// }

// fn create_node_config(
//     db_dir: PathBuf,
//     api_backend_addr: SocketAddr,
//     network_backend_port: u16,
//     nodes: Vec<NodeId>,
//     id: [u8; 32],
//     threshold: Fraction,
//     timeout: Duration,
//     mixnet_node_config: Option<MixnetNodeConfig>,
//     mixnet_topology: MixnetTopology,
// ) -> Config {
//     let mixnet_client_mode = match mixnet_node_config {
//         Some(node_config) => mixnet_client::MixnetClientMode::SenderReceiver(
//             node_config.client_listen_address.to_string(),
//         ),
//         None => mixnet_client::MixnetClientMode::Sender,
//     };

//     let mut config = Config {
//         network: NetworkConfig {
//             backend: Libp2pConfig {
//                 inner: Default::default(),
//                 initial_peers: vec![],
//                 mixnet_client: MixnetClientConfig {
//                     mode: mixnet_client_mode,
//                     topology: mixnet_topology,
//                     connection_pool_size: 255,
//                     max_retries: 3,
//                     retry_delay: Duration::from_secs(5),
//                 },
//                 mixnet_delay: Duration::ZERO..Duration::from_millis(10),
//             },
//         },
//         consensus: CarnotSettings {
//             private_key: id,
//             overlay_settings: TreeOverlaySettings {
//                 nodes,
//                 leader: RoundRobin::new(),
//                 current_leader: [0; 32].into(),
//                 number_of_committees: 1,
//                 committee_membership: RandomBeaconState::initial_sad_from_entropy([0; 32]),
//                 // By setting the threshold to 1 we ensure that all nodes come
//                 // online before progressing. This is only necessary until we add a way
//                 // to recover poast blocks from other nodes.
//                 super_majority_threshold: Some(threshold),
//             },
//             timeout,
//             transaction_selector_settings: (),
//             blob_selector_settings: (),
//         },
//         log: Default::default(),
//         http: nomos_api::ApiServiceSettings {
//             backend_settings: AxumBackendSettings {
//                 address: api_backend_addr,
//                 cors_origins: vec![],
//             },
//         },
//         da: nomos_da::Settings {
//             da_protocol: full_replication::Settings {
//                 voter: id,
//                 num_attestations: 1,
//             },
//             backend: nomos_da::backend::memory_cache::BlobCacheSettings {
//                 max_capacity: usize::MAX,
//                 evicting_period: Duration::from_secs(60 * 60 * 24), // 1 day
//             },
//         },
//         storage: RocksBackendSettings {
//             db_path: db_dir,
//             read_only: false,
//             column_family: Some("blocks".into()),
//         },
//     };

//     config.network.backend.inner.port = network_backend_port;

//     config
// }

// #[test]
// fn integration_test() {
//     let temp_dir = tempfile::tempdir().unwrap();
//     let dir = temp_dir.path().to_path_buf().join("integration_test");
//     let peer_dir = dir.join("peer");
//     // create a directory for the db
//     std::fs::create_dir_all(&peer_dir).unwrap();

//     let mut mixnet_configs = Vec::with_capacity(3);
//     // Run 3 mixnodes
//     mixnet_configs.push(run_mixnode(
//         "mixnode1",
//         "127.0.0.1:7707".parse().unwrap(),
//         "127.0.0.1:7708".parse().unwrap(),
//     ));
//     mixnet_configs.push(run_mixnode(
//         "mixnode2",
//         "127.0.0.1:7717".parse().unwrap(),
//         "127.0.0.1:7718".parse().unwrap(),
//     ));
//     mixnet_configs.push(run_mixnode(
//         "mixnode3",
//         "127.0.0.1:7727".parse().unwrap(),
//         "127.0.0.1:7728".parse().unwrap(),
//     ));
//     let mixnet_topology = build_topology(mixnet_configs.clone());

//     // Run bootstrap nomos node
//     let ids = [[0; 32], [1; 32]];
//     let config1 = create_node_config(
//         dir.clone(),
//         "127.0.0.1:11000".parse().unwrap(),
//         3000,
//         ids.iter().copied().map(NodeId::new).collect(),
//         ids[0],
//         Fraction::one(),
//         Duration::from_secs(5),
//         mixnet_configs.pop(),
//         mixnet_topology.clone(),
//     );
//     let config2 = create_node_config(
//         peer_dir.clone(),
//         "127.0.0.1:11001".parse().unwrap(),
//         3001,
//         ids.iter().copied().map(NodeId::new).collect(),
//         ids[1],
//         Fraction::one(),
//         Duration::from_secs(5),
//         mixnet_configs.pop(),
//         mixnet_topology.clone(),
//     );
//     let mut configs = vec![config1, config2];
//     let overlay = TreeOverlay::new(configs[0].consensus.overlay_settings.clone());
//     let next_leader = overlay.next_leader();
//     let next_leader_idx = ids
//         .iter()
//         .position(|&id| NodeId::from(id) == next_leader)
//         .unwrap();
//     let next_leader_config = configs.swap_remove(next_leader_idx);
//     let libp2p_config = next_leader_config.network.clone();
//     let explorer_db_dir = next_leader_config.storage.db_path.clone();
//     let node_api_addr = next_leader_config.http.backend_settings.address;
//     let prev_node_addr = nomos_libp2p::multiaddr!(
//         Ip4([127, 0, 0, 1]),
//         Tcp(next_leader_config.network.backend.inner.port)
//     );
//     run_nomos_node("bootstrap", next_leader_config);

//     configs[0]
//         .network
//         .backend
//         .initial_peers
//         .push(prev_node_addr);
//     run_nomos_node("libp2p", configs.pop().unwrap());

//     // wait for the bootstrap node to start
//     std::thread::sleep(std::time::Duration::from_secs(1));

//     run_explorer(explorer_db_dir);

//     let (rx1, app1) = run_chat(
//         "user1".into(),
//         "test.config.chat.user1.yaml",
//         format!("http://{}", node_api_addr).parse().unwrap(),
//         "http://127.0.0.1:5000".parse().unwrap(),
//         libp2p_config.clone(),
//     );
//     let (rx2, app2) = run_chat(
//         "user2".into(),
//         "test.config.chat.user2.yaml",
//         format!("http://{}", node_api_addr).parse().unwrap(),
//         "http://127.0.0.1:5000".parse().unwrap(),
//         libp2p_config,
//     );

//     app1.send_message("Hello from user1".into());
//     tracing::info!("user1: sent message: Hello from user1");

//     app2.send_message("Hello from user2".into());
//     tracing::info!("user2: sent message: Hello from user2");
//     let msgs1 = rx1.recv().unwrap();
//     let msgs2 = rx2.recv().unwrap();

//     assert_eq!(msgs1.len(), 1);
//     assert_eq!(msgs2.len(), 1);
// }
