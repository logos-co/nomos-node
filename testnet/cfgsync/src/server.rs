// std
use std::fs;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
// crates
use axum::extract::State;
use axum::Json;
use axum::{http::StatusCode, response::IntoResponse, routing::post, Router};
use nomos_da_network_core::swarm::{DAConnectionMonitorSettings, DAConnectionPolicySettings};
use nomos_tracing_service::TracingSettings;
use serde::{Deserialize, Serialize};
use tests::nodes::executor::create_executor_config;
use tests::nodes::validator::create_validator_config;
use tests::topology::configs::consensus::ConsensusParams;
use tests::topology::configs::da::DaParams;
use tokio::sync::oneshot::channel;
// internal
use crate::config::Host;
use crate::repo::{ConfigRepo, RepoResponse};

#[derive(Default, Debug, Deserialize)]
pub struct CfgSyncConfig {
    pub port: u16,
    pub n_hosts: usize,
    pub timeout: u64,

    // ConsensusConfig related parameters
    pub security_param: u32,
    pub active_slot_coeff: f64,

    // DaConfig related parameters
    pub subnetwork_size: usize,
    pub dispersal_factor: usize,
    pub num_samples: u16,
    pub num_subnets: u16,
    pub old_blobs_check_interval_secs: u64,
    pub blobs_validity_duration_secs: u64,
    pub global_params_path: String,
    pub balancer_interval_secs: u64,

    // Tracing params
    pub tracing_settings: TracingSettings,
}

impl CfgSyncConfig {
    pub fn load_from_file(file_path: &PathBuf) -> Result<Self, String> {
        let config_content = fs::read_to_string(file_path)
            .map_err(|err| format!("Failed to read config file: {}", err))?;
        serde_yaml::from_str(&config_content)
            .map_err(|err| format!("Failed to parse config file: {}", err))
    }

    pub fn to_consensus_params(&self) -> ConsensusParams {
        ConsensusParams {
            n_participants: self.n_hosts,
            security_param: self.security_param,
            active_slot_coeff: self.active_slot_coeff,
        }
    }

    pub fn to_da_params(&self) -> DaParams {
        DaParams {
            subnetwork_size: self.subnetwork_size,
            dispersal_factor: self.dispersal_factor,
            num_samples: self.num_samples,
            num_subnets: self.num_subnets,
            old_blobs_check_interval: Duration::from_secs(self.old_blobs_check_interval_secs),
            blobs_validity_duration: Duration::from_secs(self.blobs_validity_duration_secs),
            global_params_path: self.global_params_path.clone(),
            policy_settings: DAConnectionPolicySettings {
                min_dispersal_peers: self.num_subnets as usize,
                min_replication_peers: self.dispersal_factor,
                max_dispersal_failures: 0,
                max_sampling_failures: 0,
                max_replication_failures: 0,
                malicious_threshold: 0,
            },
            monitor_settings: DAConnectionMonitorSettings::default(),
            balancer_interval: Duration::from_secs(self.balancer_interval_secs),
            redial_cooldown: Duration::ZERO,
        }
    }

    pub fn to_tracing_settings(&self) -> TracingSettings {
        self.tracing_settings.clone()
    }
}

#[derive(Serialize, Deserialize)]
pub struct ClientIp {
    pub ip: Ipv4Addr,
    pub identifier: String,
}

async fn validator_config(
    State(config_repo): State<Arc<ConfigRepo>>,
    Json(payload): Json<ClientIp>,
) -> impl IntoResponse {
    let ClientIp { ip, identifier } = payload;

    let (reply_tx, reply_rx) = channel();
    config_repo.register(Host::default_validator_from_ip(ip, identifier), reply_tx);

    match reply_rx.await {
        Ok(config_response) => match config_response {
            RepoResponse::Config(config) => {
                let config = create_validator_config(*config);
                (StatusCode::OK, Json(config)).into_response()
            }
            RepoResponse::Timeout => (StatusCode::REQUEST_TIMEOUT).into_response(),
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Error receiving config").into_response(),
    }
}

async fn executor_config(
    State(config_repo): State<Arc<ConfigRepo>>,
    Json(payload): Json<ClientIp>,
) -> impl IntoResponse {
    let ClientIp { ip, identifier } = payload;

    let (reply_tx, reply_rx) = channel();
    config_repo.register(Host::default_executor_from_ip(ip, identifier), reply_tx);

    match reply_rx.await {
        Ok(config_response) => match config_response {
            RepoResponse::Config(config) => {
                let config = create_executor_config(*config);
                (StatusCode::OK, Json(config)).into_response()
            }
            RepoResponse::Timeout => (StatusCode::REQUEST_TIMEOUT).into_response(),
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Error receiving config").into_response(),
    }
}

pub fn cfgsync_app(config_repo: Arc<ConfigRepo>) -> Router {
    Router::new()
        .route("/validator", post(validator_config))
        .route("/executor", post(executor_config))
        .with_state(config_repo.clone())
}
