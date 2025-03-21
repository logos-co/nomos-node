use std::{
    net::SocketAddr,
    ops::Range,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

use cryptarchia_consensus::CryptarchiaSettings;
use cryptarchia_engine::time::SlotConfig;
use kzgrs_backend::common::share::DaShare;
use nomos_blend::{
    message_blend::{
        CryptographicProcessorSettings, MessageBlendSettings, TemporalSchedulerSettings,
    },
    persistent_transmission::PersistentTransmissionSettings,
};
use nomos_da_dispersal::{
    backend::kzgrs::{DispersalKZGRSBackendSettings, EncoderSettings},
    DispersalServiceSettings,
};
use nomos_da_indexer::{
    storage::adapters::rocksdb::RocksAdapterSettings as IndexerStorageAdapterSettings,
    IndexerSettings,
};
use nomos_da_network_service::{
    backends::libp2p::{
        common::DaNetworkBackendSettings, executor::DaNetworkExecutorBackendSettings,
    },
    NetworkConfig as DaNetworkConfig,
};
use nomos_da_sampling::{
    api::http::ApiAdapterSettings, backend::kzgrs::KzgrsSamplingBackendSettings,
    DaSamplingServiceSettings,
};
use nomos_da_verifier::{
    backend::kzgrs::KzgrsDaVerifierSettings,
    storage::adapters::rocksdb::RocksAdapterSettings as VerifierStorageAdapterSettings,
    DaVerifierServiceSettings,
};
use nomos_executor::{api::backend::AxumBackendSettings, config::Config};
use nomos_network::{backends::libp2p::Libp2pConfig, NetworkConfig};
use nomos_node::{
    api::paths::{CL_METRICS, DA_BLACKLISTED_PEERS, DA_BLOCK_PEER, DA_GET_RANGE, DA_UNBLOCK_PEER},
    config::mempool::MempoolConfig,
    RocksBackendSettings,
};
use nomos_time::{backends::system_time::SystemTimeBackendSettings, TimeServiceSettings};
use nomos_tracing::logging::local::FileConfig;
use nomos_tracing_service::LoggerLayer;
use tempfile::NamedTempFile;

use super::{create_tempdir, persist_tempdir, GetRangeReq, CLIENT};
use crate::{
    adjust_timeout, nodes::LOGS_PREFIX, topology::configs::GeneralConfig, IS_DEBUG_TRACING,
};

const BIN_PATH: &str = "../target/debug/nomos-executor";

pub struct Executor {
    addr: SocketAddr,
    tempdir: tempfile::TempDir,
    child: Child,
    config: Config,
}

impl Drop for Executor {
    fn drop(&mut self) {
        if std::thread::panicking() {
            if let Err(e) = persist_tempdir(&mut self.tempdir, "nomos-executor") {
                println!("failed to persist tempdir: {e}");
            }
        }

        if let Err(e) = self.child.kill() {
            println!("failed to kill the child process: {e}");
        }
    }
}

impl Executor {
    pub async fn spawn(mut config: Config) -> Self {
        let dir = create_tempdir().unwrap();
        let mut file = NamedTempFile::new().unwrap();
        let config_path = file.path().to_owned();

        if !*IS_DEBUG_TRACING {
            // setup logging so that we can intercept it later in testing
            config.tracing.logger = LoggerLayer::File(FileConfig {
                directory: dir.path().to_owned(),
                prefix: Some(LOGS_PREFIX.into()),
            });
        }

        config.storage.db_path = dir.path().join("db");
        dir.path().clone_into(
            &mut config
                .da_verifier
                .storage_adapter_settings
                .blob_storage_directory,
        );
        dir.path()
            .clone_into(&mut config.da_indexer.storage.blob_storage_directory);

        serde_yaml::to_writer(&mut file, &config).unwrap();
        let child = Command::new(std::env::current_dir().unwrap().join(BIN_PATH))
            .arg(&config_path)
            .current_dir(dir.path())
            .stdout(Stdio::inherit())
            .spawn()
            .unwrap();
        let node = Self {
            addr: config.http.backend_settings.address,
            child,
            tempdir: dir,
            config,
        };
        tokio::time::timeout(adjust_timeout(Duration::from_secs(10)), async {
            node.wait_online().await;
        })
        .await
        .unwrap();

        node
    }

    pub async fn get_indexer_range(
        &self,
        app_id: [u8; 32],
        range: Range<[u8; 8]>,
    ) -> Vec<([u8; 8], Vec<DaShare>)> {
        CLIENT
            .post(format!("http://{}{}", self.addr, DA_GET_RANGE))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&GetRangeReq { app_id, range }).unwrap())
            .send()
            .await
            .unwrap()
            .json::<Vec<([u8; 8], Vec<DaShare>)>>()
            .await
            .unwrap()
    }

    pub async fn block_peer(&self, peer_id: String) -> bool {
        CLIENT
            .post(format!("http://{}{}", self.addr, DA_BLOCK_PEER))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&peer_id).unwrap())
            .send()
            .await
            .unwrap()
            .json::<bool>()
            .await
            .unwrap()
    }

    pub async fn unblock_peer(&self, peer_id: String) -> bool {
        CLIENT
            .post(format!("http://{}{}", self.addr, DA_UNBLOCK_PEER))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&peer_id).unwrap())
            .send()
            .await
            .unwrap()
            .json::<bool>()
            .await
            .unwrap()
    }

    pub async fn blacklisted_peers(&self) -> Vec<String> {
        CLIENT
            .get(format!("http://{}{}", self.addr, DA_BLACKLISTED_PEERS))
            .send()
            .await
            .unwrap()
            .json::<Vec<String>>()
            .await
            .unwrap()
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

    async fn get(&self, path: &str) -> reqwest::Result<reqwest::Response> {
        CLIENT
            .get(format!("http://{}{}", self.addr, path))
            .send()
            .await
    }

    #[must_use]
    pub const fn config(&self) -> &Config {
        &self.config
    }
}

#[must_use]
#[expect(clippy::too_many_lines, reason = "TODO: Address this at some point.")]
pub fn create_executor_config(config: GeneralConfig) -> Config {
    Config {
        network: NetworkConfig {
            backend: Libp2pConfig {
                inner: config.network_config.swarm_config,
                initial_peers: config.network_config.initial_peers,
            },
        },
        blend: nomos_blend_service::BlendConfig {
            backend: config.blend_config.backend,
            persistent_transmission: PersistentTransmissionSettings::default(),
            message_blend: MessageBlendSettings {
                cryptographic_processor: CryptographicProcessorSettings {
                    private_key: config.blend_config.private_key.to_bytes(),
                    num_blend_layers: 1,
                },
                temporal_processor: TemporalSchedulerSettings {
                    max_delay: Duration::from_secs(2),
                },
            },
            cover_traffic: nomos_blend_service::CoverTrafficExtSettings {
                epoch_duration: Duration::from_secs(432_000),
                slot_duration: Duration::from_secs(20),
            },
            membership: config.blend_config.membership,
        },
        cryptarchia: CryptarchiaSettings {
            leader_config: config.consensus_config.leader_config,
            config: config.consensus_config.ledger_config,
            genesis_state: config.consensus_config.genesis_state,
            transaction_selector_settings: (),
            blob_selector_settings: (),
            network_adapter_settings:
                cryptarchia_consensus::network::adapters::libp2p::LibP2pAdapterSettings {
                    topic: String::from(nomos_node::CONSENSUS_TOPIC),
                },
            blend_adapter_settings:
                cryptarchia_consensus::blend::adapters::libp2p::LibP2pAdapterSettings {
                    broadcast_settings:
                        nomos_blend_service::network::libp2p::Libp2pBroadcastSettings {
                            topic: String::from(nomos_node::CONSENSUS_TOPIC),
                        },
                },
            recovery_file: PathBuf::from("./recovery/cryptarchia.json"),
        },
        da_network: DaNetworkConfig {
            backend: DaNetworkExecutorBackendSettings {
                validator_settings: DaNetworkBackendSettings {
                    node_key: config.da_config.node_key,
                    membership: config.da_config.membership.clone(),
                    listening_address: config.da_config.listening_address,
                    policy_settings: config.da_config.policy_settings,
                    monitor_settings: config.da_config.monitor_settings,
                    balancer_interval: config.da_config.balancer_interval,
                    redial_cooldown: config.da_config.redial_cooldown,
                    replication_settings: config.da_config.replication_settings,
                },
                num_subnets: config.da_config.num_subnets,
            },
        },
        da_indexer: IndexerSettings {
            storage: IndexerStorageAdapterSettings {
                blob_storage_directory: "./".into(),
            },
        },
        da_verifier: DaVerifierServiceSettings {
            verifier_settings: KzgrsDaVerifierSettings {
                global_params_path: config.da_config.global_params_path.clone(),
            },
            network_adapter_settings: (),
            storage_adapter_settings: VerifierStorageAdapterSettings {
                blob_storage_directory: "./".into(),
            },
        },
        tracing: config.tracing_config.tracing_settings,
        http: nomos_api::ApiServiceSettings {
            backend_settings: AxumBackendSettings {
                address: config.api_config.address,
                cors_origins: vec![],
            },
            request_timeout: None,
        },
        da_sampling: DaSamplingServiceSettings {
            sampling_settings: KzgrsSamplingBackendSettings {
                num_samples: config.da_config.num_samples,
                num_subnets: config.da_config.num_subnets,
                old_blobs_check_interval: config.da_config.old_blobs_check_interval,
                blobs_validity_duration: config.da_config.blobs_validity_duration,
            },
            api_adapter_settings: ApiAdapterSettings {
                membership: config.da_config.membership,
                api_port: config.api_config.address.port(),
                is_secure: false,
            },
        },
        storage: RocksBackendSettings {
            db_path: "./db".into(),
            read_only: false,
            column_family: Some("blocks".into()),
        },
        da_dispersal: DispersalServiceSettings {
            backend: DispersalKZGRSBackendSettings {
                encoder_settings: EncoderSettings {
                    num_columns: config.da_config.num_subnets as usize,
                    with_cache: false,
                    global_params_path: config.da_config.global_params_path,
                },
                dispersal_timeout: Duration::from_secs(20),
                mempool_strategy: config.da_config.mempool_strategy,
            },
        },
        time: TimeServiceSettings {
            backend_settings: SystemTimeBackendSettings {
                slot_config: SlotConfig {
                    slot_duration: config.time_config.slot_duration,
                    chain_start_time: config.time_config.chain_start_time,
                },
                epoch_config: config.consensus_config.ledger_config.epoch_config,
                base_period_length: config.consensus_config.ledger_config.base_period_length(),
            },
        },
        mempool: MempoolConfig {
            cl_pool_recovery_path: "./recovery/cl_mempool.json".into(),
            da_pool_recovery_path: "./recovery/da_mempool.json".into(),
        },
    }
}
