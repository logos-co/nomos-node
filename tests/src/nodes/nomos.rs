// std
use std::net::SocketAddr;
use std::ops::Range;
use std::process::{Child, Command, Stdio};
use std::str::FromStr;
use std::time::Duration;
// crates
use blst::min_sig::SecretKey;
use cl::{InputWitness, NoteWitness, NullifierSecret};
use cryptarchia_consensus::{CryptarchiaInfo, CryptarchiaSettings, TimeConfig};
use cryptarchia_ledger::LedgerState;
use kzgrs_backend::dispersal::BlobInfo;
<<<<<<< HEAD
=======
#[cfg(feature = "mixnet")]
use mixnet::{
    address::NodeAddress,
    client::MixClientConfig,
    node::MixNodeConfig,
    topology::{MixNodeInfo, MixnetTopology},
};
>>>>>>> 6060387c (Clippy happy)
use nomos_core::{block::Block, header::HeaderId, staking::NMO_UNIT};
use nomos_da_dispersal::backend::kzgrs::{DispersalKZGRSBackendSettings, EncoderSettings};
use nomos_da_dispersal::DispersalServiceSettings;
use nomos_da_indexer::storage::adapters::rocksdb::RocksAdapterSettings as IndexerStorageAdapterSettings;
use nomos_da_indexer::IndexerSettings;
use nomos_da_network_service::backends::libp2p::common::DaNetworkBackendSettings;
use nomos_da_network_service::NetworkConfig as DaNetworkConfig;
use nomos_da_sampling::backend::kzgrs::KzgrsSamplingBackendSettings;
use nomos_da_sampling::storage::adapters::rocksdb::RocksAdapterSettings as SamplingStorageAdapterSettings;
use nomos_da_sampling::DaSamplingServiceSettings;
use nomos_da_verifier::backend::kzgrs::KzgrsDaVerifierSettings;
use nomos_da_verifier::storage::adapters::rocksdb::RocksAdapterSettings as VerifierStorageAdapterSettings;
use nomos_da_verifier::DaVerifierServiceSettings;
use nomos_executor::config::Config as ExecutorConfig;
use nomos_libp2p::{Multiaddr, PeerId, SwarmConfig};
use nomos_log::LoggerBackend;
use nomos_mempool::MempoolMetrics;
<<<<<<< HEAD
=======
#[cfg(feature = "mixnet")]
use nomos_network::backends::libp2p::mixnet::MixnetConfig;
<<<<<<< HEAD
use nomos_network::backends::NetworkBackend;
>>>>>>> fb646a0e (Fill in missing gaps in test)
=======
>>>>>>> 6060387c (Clippy happy)
use nomos_network::{backends::libp2p::Libp2pConfig, NetworkConfig};
use nomos_node::api::backend::AxumBackendSettings;
use nomos_node::api::paths::{
    CL_METRICS, CRYPTARCHIA_HEADERS, CRYPTARCHIA_INFO, DA_GET_RANGE, STORAGE_BLOCK,
};
use nomos_node::{Config as ValidatorConfig, Tx};
use nomos_storage::backends::rocksdb::RocksBackendSettings;
use once_cell::sync::Lazy;
use rand::{thread_rng, Rng};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use subnetworks_assignations::versions::v1::FillFromNodeList;
use subnetworks_assignations::MembershipHandler;
use tempfile::NamedTempFile;
use time::OffsetDateTime;
// internal
use super::{
    create_tempdir, executor_config_from_node_config, persist_tempdir, ExecutorSettings,
    LOGS_PREFIX,
};
use crate::{
    adjust_timeout, get_available_port, node_address_from_port, ConsensusConfig, DaConfig, Node,
    SpawnConfig,
};

static CLIENT: Lazy<Client> = Lazy::new(Client::new);
const DEFAULT_SLOT_TIME: u64 = 2;
const CONSENSUS_SLOT_TIME_VAR: &str = "CONSENSUS_SLOT_TIME";

pub struct TestNode<Config> {
    addr: SocketAddr,
    _tempdir: tempfile::TempDir,
    child: Child,
    config: Config,
}

impl<Config> Drop for TestNode<Config> {
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
impl TestNode<ExecutorConfig> {
    const BIN_PATH: &'static str = "../target/debug/nomos-executor";
    pub async fn spawn_inner(mut config: ExecutorConfig) -> Self {
        // Waku stores the messages in a db file in the current dir, we need a different
        // directory for each node to avoid conflicts
        let dir = create_tempdir().unwrap();
        let mut file = NamedTempFile::new().unwrap();
        let config_path = file.path().to_owned();

        // setup logging so that we can intercept it later in testing
        //config.log.backend = LoggerBackend::File {
        //    directory: dir.path().to_owned(),
        //    prefix: Some(LOGS_PREFIX.into()),
        //};
        //config.log.format = LoggerFormat::Json;
        config.log.backend = LoggerBackend::Stdout;
        config.log.level = tracing::Level::INFO;

        config.storage.db_path = dir.path().join("db");
        config
            .da_sampling
            .storage_adapter_settings
            .blob_storage_directory = dir.path().to_owned();
        config
            .da_verifier
            .storage_adapter_settings
            .blob_storage_directory = dir.path().to_owned();
        config.da_indexer.storage.blob_storage_directory = dir.path().to_owned();

        serde_yaml::to_writer(&mut file, &config).unwrap();
        let child = Command::new(std::env::current_dir().unwrap().join(Self::BIN_PATH))
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
}

impl TestNode<ValidatorConfig> {
    const BIN_PATH: &'static str = "../target/debug/nomos-node";

    pub async fn spawn_inner(mut config: ValidatorConfig) -> Self {
        // Waku stores the messages in a db file in the current dir, we need a different
        // directory for each node to avoid conflicts
        let dir = create_tempdir().unwrap();
        let mut file = NamedTempFile::new().unwrap();
        let config_path = file.path().to_owned();

        // setup logging so that we can intercept it later in testing
        //config.log.backend = LoggerBackend::File {
        //    directory: dir.path().to_owned(),
        //    prefix: Some(LOGS_PREFIX.into()),
        //};
        //config.log.format = LoggerFormat::Json;
        config.log.backend = LoggerBackend::Stdout;
        config.log.level = tracing::Level::INFO;

        config.storage.db_path = dir.path().join("db");
        config
            .da_sampling
            .storage_adapter_settings
            .blob_storage_directory = dir.path().to_owned();
        config
            .da_verifier
            .storage_adapter_settings
            .blob_storage_directory = dir.path().to_owned();
        config.da_indexer.storage.blob_storage_directory = dir.path().to_owned();

        serde_yaml::to_writer(&mut file, &config).unwrap();
        let child = Command::new(std::env::current_dir().unwrap().join(Self::BIN_PATH))
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
}

impl<Config> TestNode<Config> {
    async fn get(&self, path: &str) -> reqwest::Result<reqwest::Response> {
        CLIENT
            .get(format!("http://{}{}", self.addr, path))
            .send()
            .await
    }

    pub fn url(&self) -> Url {
        format!("http://{}", self.addr).parse().unwrap()
    }

    async fn wait_online(&self) {
        loop {
            let res = self.get(CL_METRICS).await;
            if res.is_ok() && res.unwrap().status().is_success() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn get_block(&self, id: HeaderId) -> Option<Block<Tx, BlobInfo>> {
        CLIENT
            .post(format!("http://{}{}", self.addr, STORAGE_BLOCK))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&id).unwrap())
            .send()
            .await
            .unwrap()
            .json::<Option<Block<Tx, BlobInfo>>>()
            .await
            .unwrap()
    }

    pub async fn get_mempoool_metrics(&self, pool: Pool) -> MempoolMetrics {
        let discr = match pool {
            Pool::Cl => "cl",
            Pool::Da => "da",
        };
        let addr = format!("/{}/metrics", discr);
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

    pub async fn get_indexer_range(
        &self,
        app_id: [u8; 32],
        range: Range<[u8; 8]>,
    ) -> Vec<([u8; 8], Vec<Vec<u8>>)> {
        CLIENT
            .post(format!("http://{}{}", self.addr, DA_GET_RANGE))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&GetRangeReq { app_id, range }).unwrap())
            .send()
            .await
            .unwrap()
            .json::<Vec<([u8; 8], Vec<Vec<u8>>)>>()
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

    pub async fn get_headers(&self, from: Option<HeaderId>, to: Option<HeaderId>) -> Vec<HeaderId> {
        let mut req = CLIENT.get(format!("http://{}{}", self.addr, CRYPTARCHIA_HEADERS));

        if let Some(from) = from {
            req = req.query(&[("from", from)]);
        }

        if let Some(to) = to {
            req = req.query(&[("to", to)]);
        }

        let res = req.send().await;

        println!("res: {res:?}");

        res.unwrap().json::<Vec<HeaderId>>().await.unwrap()
    }
}

#[async_trait::async_trait]
impl Node for TestNode<ValidatorConfig> {
    type ConsensusInfo = CryptarchiaInfo;
    type Config = ValidatorConfig;

    async fn spawn(config: Self::Config) -> Self {
        Self::spawn_inner(config).await
    }

    async fn consensus_info(&self) -> Self::ConsensusInfo {
        let res = self.get(CRYPTARCHIA_INFO).await;
        println!("{:?}", res);
        res.unwrap().json().await.unwrap()
    }

    fn stop(&mut self) {
        self.child.kill().unwrap();
    }

    /// Depending on the network topology, the next leader must be spawned first,
    /// so the leader can receive votes from all other nodes that will be subsequently spawned.
    /// If not, the leader will miss votes from nodes spawned before itself.
    /// This issue will be resolved by devising the block catch-up mechanism in the future.
<<<<<<< HEAD
    fn create_node_configs(consensus: ConsensusConfig, da: DaConfig) -> Vec<Self::Config> {
        // we use the same random bytes for:
        // * da id
        // * coin sk
        // * coin nonce
        let mut ids = vec![[0; 32]; consensus.n_participants];
        for id in &mut ids {
            thread_rng().fill(id);
        }

        let notes = ids
            .iter()
            .map(|&id| {
                let mut sk = [0; 16];
                sk.copy_from_slice(&id[0..16]);
                InputWitness::new(
                    NoteWitness::basic(1, NMO_UNIT, &mut thread_rng()),
                    NullifierSecret(sk),
                )
            })
            .collect::<Vec<_>>();
        // no commitments for now, proofs are not checked anyway
        let genesis_state = LedgerState::from_commitments(
            notes.iter().map(|n| n.note_commitment()),
            (ids.len() as u32).into(),
        );
        let ledger_config = cryptarchia_ledger::Config {
            epoch_stake_distribution_stabilization: 3,
            epoch_period_nonce_buffer: 3,
            epoch_period_nonce_stabilization: 4,
            consensus_config: cryptarchia_engine::Config {
                security_param: consensus.security_param,
                active_slot_coeff: consensus.active_slot_coeff,
            },
        };
        let slot_duration = std::env::var(CONSENSUS_SLOT_TIME_VAR)
            .map(|s| <u64>::from_str(&s).unwrap())
            .unwrap_or(DEFAULT_SLOT_TIME);
        let time_config = TimeConfig {
            slot_duration: Duration::from_secs(slot_duration),
            chain_start_time: OffsetDateTime::now_utc(),
        };

        #[allow(unused_mut, unused_variables)]
        let mut configs = ids
            .into_iter()
            .zip(notes)
            .enumerate()
            .map(|(i, (da_id, coin))| {
                create_node_config(
                    da_id,
                    genesis_state.clone(),
                    ledger_config.clone(),
                    vec![coin],
                    time_config.clone(),
                    da.clone(),
                )
            })
            .collect::<Vec<_>>();

        // Build DA memberships and address lists.
        let peer_addresses = build_da_peer_list(&configs);
        let peer_ids = peer_addresses.iter().map(|(p, _)| *p).collect::<Vec<_>>();

        for config in &mut configs {
            let membership =
                FillFromNodeList::new(&peer_ids, da.subnetwork_size, da.dispersal_factor);
            let local_peer_id = secret_key_to_peer_id(config.da_network.backend.node_key.clone());
            let subnetwork_ids = membership.membership(&local_peer_id);
            config.da_verifier.verifier_settings.index = subnetwork_ids;
            config.da_network.backend.membership = membership;
            config.da_network.backend.addresses = peer_addresses.iter().cloned().collect();
        }

        configs
    }
=======
>>>>>>> fb646a0e (Fill in missing gaps in test)

    fn node_configs(config: SpawnConfig) -> Vec<Self::Config> {
        match config {
            SpawnConfig::Star { consensus, da, .. } => {
                let mut configs = Self::create_node_configs(consensus, da);
                let next_leader_config = configs.remove(0);
                let first_node_addr =
                    node_address_from_port(next_leader_config.network.backend.inner.port);
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
            SpawnConfig::Chain { consensus, da, .. } => {
                let mut configs = Self::create_node_configs(consensus, da);
                let next_leader_config = configs.remove(0);
                let mut prev_node_addr =
                    node_address_from_port(next_leader_config.network.backend.inner.port);
                let mut node_configs = vec![next_leader_config];
                for mut conf in configs {
                    conf.network.backend.initial_peers.push(prev_node_addr);
                    prev_node_addr = node_address_from_port(conf.network.backend.inner.port);

                    node_configs.push(conf);
                }
                node_configs
            }
        }
    }

    fn create_node_configs(consensus: ConsensusConfig, da: DaConfig) -> Vec<Self::Config> {
        __create_node_configs(consensus, da)
    }
}

#[async_trait::async_trait]
impl Node for TestNode<ExecutorConfig> {
    type ConsensusInfo = CryptarchiaInfo;
    type Config = ExecutorConfig;

    async fn spawn(config: Self::Config) -> Self {
        Self::spawn_inner(config).await
    }

    async fn consensus_info(&self) -> Self::ConsensusInfo {
        let res = self.get(CRYPTARCHIA_INFO).await;
        println!("{:?}", res);
        res.unwrap().json().await.unwrap()
    }

    fn stop(&mut self) {
        self.child.kill().unwrap();
    }

    /// Depending on the network topology, the next leader must be spawned first,
    /// so the leader can receive votes from all other nodes that will be subsequently spawned.
    /// If not, the leader will miss votes from nodes spawned before itself.
    /// This issue will be resolved by devising the block catch-up mechanism in the future.

    fn node_configs(config: SpawnConfig) -> Vec<Self::Config> {
        match config {
            SpawnConfig::Star { consensus, da, .. } => {
                let mut configs = Self::create_node_configs(consensus, da);
                let next_leader_config = configs.remove(0);
                let first_node_addr =
                    node_address_from_port(next_leader_config.network.backend.inner.port);
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
            SpawnConfig::Chain { consensus, da, .. } => {
                let mut configs = Self::create_node_configs(consensus, da);
                let next_leader_config = configs.remove(0);
                let mut prev_node_addr =
                    node_address_from_port(next_leader_config.network.backend.inner.port);
                let mut node_configs = vec![next_leader_config];
                for mut conf in configs {
                    conf.network.backend.initial_peers.push(prev_node_addr);
                    prev_node_addr = node_address_from_port(conf.network.backend.inner.port);

                    node_configs.push(conf);
                }
                node_configs
            }
        }
    }

    fn create_node_configs(consensus: ConsensusConfig, da: DaConfig) -> Vec<Self::Config> {
        let validator_config = __create_node_configs(consensus, da.clone());
        let executor_settings = ExecutorSettings {
            da_dispersal: DispersalServiceSettings {
                backend: DispersalKZGRSBackendSettings {
                    encoder_settings: EncoderSettings {
                        num_columns: da.num_subnets as usize,
                        with_cache: false,
                        global_params_path: da.global_params_path,
                    },
                    dispersal_timeout: Duration::from_secs(u64::MAX),
                },
            },
            num_subnets: da.num_subnets,
        };
        validator_config
            .into_iter()
            .map(move |c| executor_config_from_node_config(c, executor_settings.clone()))
            .collect()
    }
}

pub enum NetworkNode {
    Validator(TestNode<ValidatorConfig>),
    Executor(TestNode<ExecutorConfig>),
}

pub enum NetworkNodeConfig {
    Validator(ValidatorConfig),
    Executor(ExecutorConfig),
}

impl NetworkNode {
    pub fn config(&self) -> NetworkNodeConfig {
        match self {
            NetworkNode::Validator(node) => NetworkNodeConfig::Validator(node.config().clone()),
            NetworkNode::Executor(node) => NetworkNodeConfig::Executor(node.config.clone()),
        }
    }

    pub async fn get_indexer_range(
        &self,
        app_id: [u8; 32],
        range: Range<[u8; 8]>,
    ) -> Vec<([u8; 8], Vec<Vec<u8>>)> {
        match self {
            NetworkNode::Validator(node) => node.get_indexer_range(app_id, range).await,
            NetworkNode::Executor(node) => node.get_indexer_range(app_id, range).await,
        }
    }
}

#[async_trait::async_trait]
impl Node for NetworkNode {
    type Config = NetworkNodeConfig;
    type ConsensusInfo = CryptarchiaInfo;

    async fn spawn(config: Self::Config) -> Self {
        match config {
            NetworkNodeConfig::Validator(config) => {
                NetworkNode::Validator(TestNode::<ValidatorConfig>::spawn(config).await)
            }
            NetworkNodeConfig::Executor(config) => {
                NetworkNode::Executor(TestNode::<ExecutorConfig>::spawn(config).await)
            }
        }
    }

    fn node_configs(config: SpawnConfig) -> Vec<Self::Config> {
        let n_executors = config.n_executors();
        let da = config.da_config();
        let validator_configs = <TestNode<ValidatorConfig> as Node>::node_configs(config);
        let (executor_configs, validator_configs) = validator_configs.split_at(n_executors);
        let executor_settings = ExecutorSettings {
            da_dispersal: DispersalServiceSettings {
                backend: DispersalKZGRSBackendSettings {
                    encoder_settings: EncoderSettings {
                        num_columns: da.num_subnets as usize,
                        with_cache: false,
                        global_params_path: da.global_params_path,
                    },
                    dispersal_timeout: Duration::from_secs(u64::MAX),
                },
            },
            num_subnets: da.num_subnets,
        };
        let validator_configs = validator_configs
            .iter()
            .cloned()
            .map(NetworkNodeConfig::Validator);
        let executor_configs = executor_configs
            .iter()
            .cloned()
            .map(move |validatorconfig| {
                executor_config_from_node_config(validatorconfig, executor_settings.clone())
            })
            .map(NetworkNodeConfig::Executor);
        executor_configs.chain(validator_configs).collect()
    }

    fn create_node_configs(_consensus: ConsensusConfig, _da: DaConfig) -> Vec<Self::Config> {
        unreachable!()
    }

    async fn consensus_info(&self) -> Self::ConsensusInfo {
        let res = match self {
            NetworkNode::Validator(node) => node.get(CRYPTARCHIA_INFO).await,
            NetworkNode::Executor(node) => node.get(CRYPTARCHIA_INFO).await,
        };
        println!("{:?}", res);
        res.unwrap().json().await.unwrap()
    }

    fn stop(&mut self) {
        let child = match self {
            NetworkNode::Validator(TestNode { child, .. }) => child,
            NetworkNode::Executor(TestNode { child, .. }) => child,
        };
        child.kill().unwrap();
    }
}

pub enum Pool {
    Da,
    Cl,
}

#[derive(Serialize, Deserialize)]
struct GetRangeReq {
    pub app_id: [u8; 32],
    pub range: Range<[u8; 8]>,
}

fn secret_key_to_peer_id(node_key: nomos_libp2p::ed25519::SecretKey) -> PeerId {
    PeerId::from_public_key(
        &nomos_libp2p::ed25519::Keypair::from(node_key)
            .public()
            .into(),
    )
}

fn build_da_peer_list(configs: &[ValidatorConfig]) -> Vec<(PeerId, Multiaddr)> {
    configs
        .iter()
        .map(|c| {
            let peer_id = secret_key_to_peer_id(c.da_network.backend.node_key.clone());
            (
                peer_id,
                c.da_network
                    .backend
                    .listening_address
                    .clone()
                    .with_p2p(peer_id)
                    .unwrap(),
            )
        })
        .collect()
}

fn __create_node_configs(consensus: ConsensusConfig, da: DaConfig) -> Vec<ValidatorConfig> {
    // we use the same random bytes for:
    // * da id
    // * coin sk
    // * coin nonce
    let mut ids = vec![[0; 32]; consensus.n_participants];
    for id in &mut ids {
        thread_rng().fill(id);
    }

    #[cfg(feature = "mixnet")]
    let (mixclient_config, mixnode_configs) = create_mixnet_config(&ids);

    let notes = ids
        .iter()
        .map(|&id| {
            let mut sk = [0; 16];
            sk.copy_from_slice(&id[0..16]);
            InputWitness::new(
                NoteWitness::basic(1, NMO_UNIT, &mut thread_rng()),
                NullifierSecret(sk),
            )
        })
        .collect::<Vec<_>>();
    // no commitments for now, proofs are not checked anyway
    let genesis_state = LedgerState::from_commitments(
        notes.iter().map(|n| n.note_commitment()),
        (ids.len() as u32).into(),
    );
    let ledger_config = cryptarchia_ledger::Config {
        epoch_stake_distribution_stabilization: 3,
        epoch_period_nonce_buffer: 3,
        epoch_period_nonce_stabilization: 4,
        consensus_config: cryptarchia_engine::Config {
            security_param: consensus.security_param,
            active_slot_coeff: consensus.active_slot_coeff,
        },
    };
    let slot_duration = std::env::var(CONSENSUS_SLOT_TIME_VAR)
        .map(|s| <u64>::from_str(&s).unwrap())
        .unwrap_or(DEFAULT_SLOT_TIME);
    let time_config = TimeConfig {
        slot_duration: Duration::from_secs(slot_duration),
        chain_start_time: OffsetDateTime::now_utc(),
    };

    #[allow(unused_mut, unused_variables)]
    let mut configs = ids
        .into_iter()
        .zip(notes)
        .enumerate()
        .map(|(i, (da_id, coin))| {
            create_node_config(
                da_id,
                genesis_state.clone(),
                ledger_config.clone(),
                vec![coin],
                time_config.clone(),
                da.clone(),
                #[cfg(feature = "mixnet")]
                MixnetConfig {
                    mixclient: mixclient_config.clone(),
                    mixnode: mixnode_configs[i].clone(),
                },
            )
        })
        .collect::<Vec<_>>();

    // Build DA memberships and address lists.
    let peer_addresses = build_da_peer_list(&configs);
    let peer_ids = peer_addresses.iter().map(|(p, _)| *p).collect::<Vec<_>>();

    for config in &mut configs {
        let membership = FillFromNodeList::new(&peer_ids, da.subnetwork_size, da.dispersal_factor);
        let local_peer_id = secret_key_to_peer_id(config.da_network.backend.node_key.clone());
        let subnetwork_ids = membership.membership(&local_peer_id);
        config.da_verifier.verifier_settings.index = subnetwork_ids;
        config.da_network.backend.membership = membership;
        config.da_network.backend.addresses = peer_addresses.iter().cloned().collect();
    }

    #[cfg(feature = "mixnet")]
    {
        // Build a topology using only a subset of nodes.
        let mixnode_candidates = configs
            .iter()
            .take(NUM_MIXNODE_CANDIDATES)
            .collect::<Vec<_>>();
        let topology = build_mixnet_topology(&mixnode_candidates);

        // Set the topology to all configs
        for config in &mut configs {
            config.network.backend.mixnet.mixclient.topology = topology.clone();
        }
        configs
    }
    #[cfg(not(feature = "mixnet"))]
    configs
}

#[allow(clippy::too_many_arguments)]
fn create_node_config(
    id: [u8; 32],
    genesis_state: LedgerState,
    config: cryptarchia_ledger::Config,
    notes: Vec<InputWitness>,
    time: TimeConfig,
    da_config: DaConfig,
) -> ValidatorConfig {
    let swarm_config: SwarmConfig = Default::default();
    let node_key = swarm_config.node_key.clone();

    let verifier_sk = SecretKey::key_gen(&id, &[]).unwrap();
    let verifier_sk_bytes = verifier_sk.to_bytes();

    let mut config = ValidatorConfig {
        network: NetworkConfig {
            backend: Libp2pConfig {
                inner: swarm_config,
                initial_peers: vec![],
            },
        },
        cryptarchia: CryptarchiaSettings {
            notes,
            config,
            genesis_state,
            time,
            transaction_selector_settings: (),
            blob_selector_settings: (),
        },
        da_network: DaNetworkConfig {
            backend: DaNetworkBackendSettings {
                node_key,
                membership: Default::default(),
                addresses: Default::default(),
                listening_address: Multiaddr::from_str(&format!(
                    "/ip4/127.0.0.1/udp/{}/quic-v1",
                    get_available_port(),
                ))
                .unwrap(),
            },
        },
        da_indexer: IndexerSettings {
            storage: IndexerStorageAdapterSettings {
                blob_storage_directory: "./".into(),
            },
        },
        da_verifier: DaVerifierServiceSettings {
            verifier_settings: KzgrsDaVerifierSettings {
                sk: hex::encode(verifier_sk_bytes),
                index: Default::default(),
                global_params_path: da_config.global_params_path,
            },
            network_adapter_settings: (),
            storage_adapter_settings: VerifierStorageAdapterSettings {
                blob_storage_directory: "./".into(),
            },
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
        da_sampling: DaSamplingServiceSettings {
            sampling_settings: KzgrsSamplingBackendSettings {
                num_samples: da_config.num_samples,
                num_subnets: da_config.num_subnets,
                old_blobs_check_interval: da_config.old_blobs_check_interval,
                blobs_validity_duration: da_config.blobs_validity_duration,
            },
            storage_adapter_settings: SamplingStorageAdapterSettings {
                blob_storage_directory: "./".into(),
            },
            network_adapter_settings: (),
        },
        storage: RocksBackendSettings {
            db_path: "./db".into(),
            read_only: false,
            column_family: Some("blocks".into()),
        },
    };

    config.network.backend.inner.port = get_available_port();

    config
}
