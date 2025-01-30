// std
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::process;
use std::sync::Arc;
use std::time::Duration;
// crates
use axum::extract::State;
use axum::Json;
use axum::{http::StatusCode, response::IntoResponse, routing::post, Router};
use cfgsync::config::Host;
use cfgsync::repo::{ConfigRepo, RepoResponse};
use cfgsync::CfgSyncConfig;
use clap::Parser;
use serde::{Deserialize, Serialize};
use tests::nodes::executor::create_executor_config;
use tests::nodes::validator::create_validator_config;
use tokio::sync::oneshot::channel;
// internal

#[derive(Parser, Debug)]
#[command(about = "CfgSync")]
struct Args {
    config: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct ClientIp {
    ip: Ipv4Addr,
    identifier: String,
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

#[tokio::main]
async fn main() {
    let cli = Args::parse();

    let config = CfgSyncConfig::load_from_file(&cli.config).unwrap_or_else(|err| {
        eprintln!("{}", err);
        process::exit(1);
    });
    let consensus_params = config.to_consensus_params();
    let da_params = config.to_da_params();
    let tracing_settings = config.to_tracing_settings();

    let config_repo = ConfigRepo::new(
        config.n_hosts,
        consensus_params,
        da_params,
        tracing_settings,
        Duration::from_secs(config.timeout),
    );
    let app = Router::new()
        .route("/validator", post(validator_config))
        .route("/executor", post(executor_config))
        .with_state(config_repo.clone());

    println!("Server running on http://0.0.0.0:{}", config.port);
    axum::Server::bind(&format!("0.0.0.0:{}", config.port).parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}
