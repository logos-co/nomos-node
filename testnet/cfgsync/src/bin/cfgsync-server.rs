// std
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::{fs, process};
// crates
use axum::extract::State;
use axum::Json;
use axum::{http::StatusCode, response::IntoResponse, routing::post, Router};
use cfgsync::config::Host;
use cfgsync::repo::{ConfigRepo, RepoResponse};
use clap::Parser;
use serde::{Deserialize, Serialize};
use tests::{ConsensusConfig, DaConfig};
use tokio::sync::oneshot::channel;
// internal

#[derive(Parser, Debug)]
#[command(about = "CfgSync")]
struct Args {
    config: PathBuf,
}

#[derive(Debug, Deserialize)]
struct CfgSyncConfig {
    port: u16,
    n_hosts: usize,
    timeout: u64,

    // ConsensusConfig related parameters
    security_param: u32,
    active_slot_coeff: f64,

    // DaConfig related parameters
    subnetwork_size: usize,
    dispersal_factor: usize,
    num_samples: u16,
    num_subnets: u16,
    old_blobs_check_interval_secs: u64,
    blobs_validity_duration_secs: u64,
    global_params_path: String,
}

impl CfgSyncConfig {
    fn load_from_file(file_path: &PathBuf) -> Result<Self, String> {
        let config_content = fs::read_to_string(file_path)
            .map_err(|err| format!("Failed to read config file: {}", err))?;
        serde_yaml::from_str(&config_content)
            .map_err(|err| format!("Failed to parse config file: {}", err))
    }

    fn to_consensus_config(&self) -> ConsensusConfig {
        ConsensusConfig {
            n_participants: self.n_hosts,
            security_param: self.security_param,
            active_slot_coeff: self.active_slot_coeff,
        }
    }

    fn to_da_config(&self) -> DaConfig {
        DaConfig {
            subnetwork_size: self.subnetwork_size,
            dispersal_factor: self.dispersal_factor,
            num_samples: self.num_samples,
            num_subnets: self.num_subnets,
            old_blobs_check_interval: Duration::from_secs(self.old_blobs_check_interval_secs),
            blobs_validity_duration: Duration::from_secs(self.blobs_validity_duration_secs),
            global_params_path: self.global_params_path.clone(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct ClientIp {
    ip: Ipv4Addr,
}

async fn node_config(
    State(config_repo): State<Arc<ConfigRepo>>,
    Json(payload): Json<ClientIp>,
) -> impl IntoResponse {
    let ClientIp { ip } = payload;

    let (reply_tx, reply_rx) = channel();
    config_repo.register(Host::default_node_from_ip(ip), reply_tx);

    match reply_rx.await {
        Ok(config_response) => match config_response {
            RepoResponse::Config(config) => (StatusCode::OK, Json(config)).into_response(),
            RepoResponse::Timeout => {
                (StatusCode::REQUEST_TIMEOUT, Json(RepoResponse::Timeout)).into_response()
            }
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Error receiving config").into_response(),
    }
}

#[tokio::main]
async fn main() {
    let cli = Args::parse();

    let config = CfgSyncConfig::load_from_file(&cli.config).unwrap_or_else(|err| {
        eprintln!("{}", err);
        process::exit(1);
    });
    let consensus_config = config.to_consensus_config();
    let da_config = config.to_da_config();

    let config_repo = ConfigRepo::new(
        config.n_hosts,
        consensus_config,
        da_config,
        Duration::from_secs(config.timeout),
    );
    let app = Router::new()
        .route("/node", post(node_config))
        .with_state(config_repo.clone());

    println!("Server running on http://0.0.0.0:{}", config.port);
    axum::Server::bind(&format!("0.0.0.0:{}", config.port).parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}
