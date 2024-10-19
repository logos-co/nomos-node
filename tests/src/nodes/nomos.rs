use std::fs::File;
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
use nomos_core::{block::Block, header::HeaderId, staking::NMO_UNIT};
use nomos_da_dispersal::backend::kzgrs::{DispersalKZGRSBackendSettings, EncoderSettings};
use nomos_da_dispersal::DispersalServiceSettings;
use nomos_da_indexer::storage::adapters::rocksdb::RocksAdapterSettings as IndexerStorageAdapterSettings;
use nomos_da_indexer::IndexerSettings;
use nomos_da_network_service::backends::libp2p::common::DaNetworkBackendSettings;
use nomos_da_network_service::backends::libp2p::executor::DaNetworkExecutorBackendSettings;
use nomos_da_network_service::NetworkConfig as DaNetworkConfig;
use nomos_da_sampling::backend::kzgrs::KzgrsSamplingBackendSettings;
use nomos_da_sampling::storage::adapters::rocksdb::RocksAdapterSettings as SamplingStorageAdapterSettings;
use nomos_da_sampling::DaSamplingServiceSettings;
use nomos_da_verifier::backend::kzgrs::KzgrsDaVerifierSettings;
use nomos_da_verifier::storage::adapters::rocksdb::RocksAdapterSettings as VerifierStorageAdapterSettings;
use nomos_da_verifier::DaVerifierServiceSettings;
use nomos_executor::api::backend::AxumBackendSettings;
use nomos_executor::config::Config as ExecutorConfig;
use nomos_libp2p::{Multiaddr, PeerId, SwarmConfig};
use nomos_log::{LoggerBackend, LoggerFormat};
use nomos_mempool::MempoolMetrics;
use nomos_network::{backends::libp2p::Libp2pConfig, NetworkConfig};
use nomos_node::api::paths::{
    CL_METRICS, CRYPTARCHIA_HEADERS, CRYPTARCHIA_INFO, DA_GET_RANGE, STORAGE_BLOCK,
};
use nomos_node::{Config, Tx};
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
use super::{create_tempdir, persist_tempdir, LOGS_PREFIX};
use crate::{adjust_timeout, get_available_port, ConsensusConfig, DaConfig, Node};

static CLIENT: Lazy<Client> = Lazy::new(Client::new);
const NOMOS_BIN: &str = "../target/debug/nomos-executor";
const DEFAULT_SLOT_TIME: u64 = 2;
const CONSENSUS_SLOT_TIME_VAR: &str = "CONSENSUS_SLOT_TIME";

pub struct NomosNode {
    addr: SocketAddr,
    _tempdir: tempfile::TempDir,
    child: Child,
    config: ExecutorConfig,
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

    pub fn config(&self) -> &ExecutorConfig {
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
impl Node for NomosNode {
    type ConsensusInfo = CryptarchiaInfo;

    async fn spawn(config: ExecutorConfig) -> Self {
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
    fn create_node_configs(consensus: ConsensusConfig, da: DaConfig) -> Vec<ExecutorConfig> {
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
            let local_peer_id = secret_key_to_peer_id(
                config
                    .da_network
                    .backend
                    .validator_settings
                    .node_key
                    .clone(),
            );
            let subnetwork_ids = membership.membership(&local_peer_id);
            config.da_verifier.verifier_settings.index = subnetwork_ids;
            config.da_network.backend.validator_settings.membership = membership;
            config.da_network.backend.validator_settings.addresses =
                peer_addresses.iter().cloned().collect();
        }

        configs
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

fn build_da_peer_list(configs: &[ExecutorConfig]) -> Vec<(PeerId, Multiaddr)> {
    configs
        .iter()
        .map(|c| {
            let peer_id =
                secret_key_to_peer_id(c.da_network.backend.validator_settings.node_key.clone());
            (
                peer_id,
                c.da_network
                    .backend
                    .validator_settings
                    .listening_address
                    .clone()
                    .with_p2p(peer_id)
                    .unwrap(),
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn create_node_config(
    id: [u8; 32],
    genesis_state: LedgerState,
    config: cryptarchia_ledger::Config,
    notes: Vec<InputWitness>,
    time: TimeConfig,
    da_config: DaConfig,
) -> ExecutorConfig {
    let swarm_config: SwarmConfig = Default::default();
    let node_key = swarm_config.node_key.clone();

    let verifier_sk = SecretKey::key_gen(&id, &[]).unwrap();
    let verifier_sk_bytes = verifier_sk.to_bytes();

    let mut config = ExecutorConfig {
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
        da_dispersal: DispersalServiceSettings {
            backend: DispersalKZGRSBackendSettings {
                encoder_settings: EncoderSettings {
                    num_columns: da_config.num_subnets as usize,
                    with_cache: false,
                    global_params_path: da_config.global_params_path.clone(),
                },
                dispersal_timeout: Duration::from_secs(10),
            },
        },
        da_network: DaNetworkConfig {
            backend: DaNetworkExecutorBackendSettings {
                validator_settings: DaNetworkBackendSettings {
                    node_key,
                    listening_address: Multiaddr::from_str(&format!(
                        "/ip4/127.0.0.1/udp/{}/quic-v1",
                        get_available_port(),
                    ))
                    .unwrap(),
                    addresses: Default::default(),
                    membership: Default::default(),
                },
                num_subnets: da_config.num_subnets as u16,
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
