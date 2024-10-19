use std::ops::Range;
use std::process::{Command, Stdio};
use std::time::Duration;
use std::{net::SocketAddr, process::Child};

use cryptarchia_consensus::{CryptarchiaInfo, CryptarchiaSettings};
use nomos_core::block::Block;
use nomos_da_indexer::storage::adapters::rocksdb::RocksAdapterSettings as IndexerStorageAdapterSettings;
use nomos_da_indexer::IndexerSettings;
use nomos_da_network_service::backends::libp2p::common::DaNetworkBackendSettings;
use nomos_da_network_service::NetworkConfig as DaNetworkConfig;
use nomos_da_sampling::storage::adapters::rocksdb::RocksAdapterSettings as SamplingStorageAdapterSettings;
use nomos_da_sampling::{backend::kzgrs::KzgrsSamplingBackendSettings, DaSamplingServiceSettings};
use nomos_da_verifier::storage::adapters::rocksdb::RocksAdapterSettings as VerifierStorageAdapterSettings;
use nomos_da_verifier::{backend::kzgrs::KzgrsDaVerifierSettings, DaVerifierServiceSettings};
use nomos_log::{LoggerBackend, LoggerFormat};
use nomos_mempool::MempoolMetrics;
use nomos_network::{backends::libp2p::Libp2pConfig, NetworkConfig};
use nomos_node::api::paths::{
    CL_METRICS, CRYPTARCHIA_HEADERS, CRYPTARCHIA_INFO, DA_GET_RANGE, STORAGE_BLOCK,
};
use nomos_node::{api::backend::AxumBackendSettings, Config, RocksBackendSettings};
use nomos_node::{BlobInfo, HeaderId, Tx};
use reqwest::Url;
use tempfile::NamedTempFile;

use crate::nodes::LOGS_PREFIX;
use crate::topology::configs::consensus::GeneralConsensusConfig;
use crate::topology::configs::da::GeneralDaConfig;
use crate::topology::configs::network::GeneralNetworkConfig;
use crate::{adjust_timeout, get_available_port};

use super::{create_tempdir, persist_tempdir, GetRangeReq, CLIENT};

const BIN_PATH: &str = "../target/debug/nomos-node";

pub enum Pool {
    Da,
    Cl,
}

pub struct ValidatorNode {
    addr: SocketAddr,
    tempdir: tempfile::TempDir,
    child: Child,
    config: Config,
}

impl Drop for ValidatorNode {
    fn drop(&mut self) {
        if std::thread::panicking() {
            if let Err(e) = persist_tempdir(&mut self.tempdir, "nomos-node") {
                println!("failed to persist tempdir: {e}");
            }
        }

        if let Err(e) = self.child.kill() {
            println!("failed to kill the child process: {e}");
        }
    }
}

impl ValidatorNode {
    pub async fn spawn(mut config: Config) -> Self {
        let dir = create_tempdir().unwrap();
        let mut file = NamedTempFile::new().unwrap();
        let config_path = file.path().to_owned();

        // setup logging so that we can intercept it later in testing
        config.log.backend = LoggerBackend::File {
            directory: dir.path().to_owned(),
            prefix: Some(LOGS_PREFIX.into()),
        };
        config.log.format = LoggerFormat::Json;

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
            self.tempdir.path().display()
        );
        // std::thread::sleep(std::time::Duration::from_secs(50));
        std::fs::read_dir(self.tempdir.path())
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

    pub async fn consensus_info(&self) -> CryptarchiaInfo {
        let res = self.get(CRYPTARCHIA_INFO).await;
        println!("{:?}", res);
        res.unwrap().json().await.unwrap()
    }
}

pub fn create_validator_config(
    consensus_config: GeneralConsensusConfig,
    da_config: GeneralDaConfig,
    network_config: GeneralNetworkConfig,
) -> Config {
    Config {
        network: NetworkConfig {
            backend: Libp2pConfig {
                inner: network_config.swarm_config,
                initial_peers: network_config.initial_peers,
            },
        },
        cryptarchia: CryptarchiaSettings {
            notes: consensus_config.notes,
            config: consensus_config.ledger_config,
            genesis_state: consensus_config.genesis_state,
            time: consensus_config.time,
            transaction_selector_settings: (),
            blob_selector_settings: (),
        },
        da_network: DaNetworkConfig {
            backend: DaNetworkBackendSettings {
                node_key: da_config.node_key,
                membership: da_config.membership,
                addresses: da_config.addresses,
                listening_address: da_config.listening_address,
            },
        },
        da_indexer: IndexerSettings {
            storage: IndexerStorageAdapterSettings {
                blob_storage_directory: "./".into(),
            },
        },
        da_verifier: DaVerifierServiceSettings {
            verifier_settings: KzgrsDaVerifierSettings {
                sk: da_config.verifier_sk,
                index: da_config.verifier_index,
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
    }
}
