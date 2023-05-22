mod bridges;
mod tx;

use clap::Parser;
use color_eyre::eyre::{eyre, Result};
use nomos_consensus::{
    network::adapters::waku::WakuAdapter as ConsensusWakuAdapter, overlay::FlatRoundRobin,
    CarnotConsensus,
};
use nomos_core::fountain::mock::MockFountain;
use nomos_http::backends::axum::AxumBackend;
use nomos_http::bridge::{HttpBridge, HttpBridgeService, HttpBridgeSettings};
use nomos_http::http::HttpService;
use nomos_log::Logger;
use nomos_mempool::{
    backend::mockpool::MockPool, network::adapters::waku::WakuAdapter as MempoolWakuAdapter,
    MempoolService,
};
use nomos_network::{backends::waku::Waku, NetworkService};
use overwatch_derive::*;
use overwatch_rs::{
    overwatch::*,
    services::{handle::ServiceHandle, ServiceData},
};
use serde::Deserialize;
use std::sync::Arc;
use tx::Tx;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path for a yaml-encoded network config file
    config: std::path::PathBuf,
}

type Carnot = CarnotConsensus<
    ConsensusWakuAdapter,
    MockPool<Tx>,
    MempoolWakuAdapter<Tx>,
    MockFountain,
    FlatRoundRobin,
>;

#[derive(Deserialize)]
struct Config {
    log: <Logger as ServiceData>::Settings,
    network: <NetworkService<Waku> as ServiceData>::Settings,
    http: <HttpService<AxumBackend> as ServiceData>::Settings,
    consensus: <Carnot as ServiceData>::Settings,
}

#[derive(Services)]
struct MockPoolNode {
    logging: ServiceHandle<Logger>,
    network: ServiceHandle<NetworkService<Waku>>,
    mockpool: ServiceHandle<MempoolService<MempoolWakuAdapter<Tx>, MockPool<Tx>>>,
    consensus: ServiceHandle<Carnot>,
    http: ServiceHandle<HttpService<AxumBackend>>,
    bridges: ServiceHandle<HttpBridgeService>,
}
/// Mockpool node
/// Minimal configuration file:
///
/// ```yaml
/// log:
///   backend: "Stdout"
///   format: "Json"
///   level: "debug"
/// network:
///   backend:
///     host: 0.0.0.0
///     port: 3000
///     log_level: "fatal"
///     nodeKey: null
///     discV5BootstrapNodes: []
///     initial_peers: []
/// http:
///   backend:
///     address: 0.0.0.0:8080
///     cors_origins: []
///
/// ```
fn main() -> Result<()> {
    let Args { config } = Args::parse();
    let config = serde_yaml::from_reader::<_, Config>(std::fs::File::open(config)?)?;
    let bridges: Vec<HttpBridge> = vec![
        Arc::new(Box::new(bridges::mempool_add_tx_bridge)),
        Arc::new(Box::new(bridges::mempool_metrics_bridge)),
        Arc::new(Box::new(bridges::waku_add_conn_bridge)),
        Arc::new(Box::new(bridges::waku_info_bridge)),
    ];
    let app = OverwatchRunner::<MockPoolNode>::run(
        MockPoolNodeServiceSettings {
            network: config.network,
            logging: config.log,
            http: config.http,
            mockpool: (),
            consensus: config.consensus,
            bridges: HttpBridgeSettings { bridges },
        },
        None,
    )
    .map_err(|e| eyre!("Error encountered: {}", e))?;
    app.wait_finished();
    Ok(())
}
