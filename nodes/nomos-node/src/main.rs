use full_replication::{Blob, Certificate};
use nomos_node::{
    Config, ConsensusArgs, HttpArgs, LogArgs, NetworkArgs, Nomos, NomosServiceSettings,
    OverlayArgs, Tx,
};

mod bridges;

use clap::Parser;
use color_eyre::eyre::{eyre, Result};
use nomos_core::{
    da::{blob, certificate},
    tx::Transaction,
};
use nomos_http::bridge::{HttpBridge, HttpBridgeSettings};
use nomos_mempool::network::adapters::libp2p::{Libp2pAdapter, Settings as AdapterSettings};
use nomos_network::backends::libp2p::Libp2p;
use overwatch_rs::overwatch::*;
use std::sync::Arc;

const DEFAULT_DB_PATH: &str = "./db";

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path for a yaml-encoded network config file
    config: std::path::PathBuf,
    /// Overrides log config.
    #[clap(flatten)]
    log_args: LogArgs,
    /// Overrides network config.
    #[clap(flatten)]
    network_args: NetworkArgs,
    /// Overrides http config.
    #[clap(flatten)]
    http_args: HttpArgs,
    /// Overrides consensus config.
    #[clap(flatten)]
    consensus_args: ConsensusArgs,
    /// Overrides overlay config.
    #[clap(flatten)]
    overlay_args: OverlayArgs,
}

fn main() -> Result<()> {
    let Args {
        config,
        log_args,
        http_args,
        network_args,
        consensus_args,
        overlay_args,
    } = Args::parse();
    let config = serde_yaml::from_reader::<_, Config>(std::fs::File::open(config)?)?
        .update_log(log_args)?
        .update_http(http_args)?
        .update_consensus(consensus_args)?
        .update_overlay(overlay_args)?
        .update_network(network_args)?;

    let bridges: Vec<HttpBridge> = vec![
        Arc::new(Box::new(bridges::carnot_info_bridge)),
        // Due to a limitation in the current api system, we can't connect a single endopint to multiple services
        // which means we need two different paths for complete mempool metrics.
        Arc::new(Box::new(bridges::cl_mempool_metrics_bridge)),
        Arc::new(Box::new(bridges::da_mempool_metrics_bridge)),
        Arc::new(Box::new(bridges::cl_mempool_status_bridge)),
        Arc::new(Box::new(bridges::da_mempool_status_bridge)),
        Arc::new(Box::new(bridges::da_blob_get_bridge)),
        Arc::new(Box::new(bridges::network_info_bridge)),
        Arc::new(Box::new(
            bridges::mempool_add_tx_bridge::<Libp2p, Libp2pAdapter<Tx, <Tx as Transaction>::Hash>>,
        )),
        Arc::new(Box::new(
            bridges::mempool_add_cert_bridge::<
                Libp2p,
                Libp2pAdapter<Certificate, <Blob as blob::Blob>::Hash>,
            >,
        )),
    ];
    let app = OverwatchRunner::<Nomos>::run(
        NomosServiceSettings {
            network: config.network,
            logging: config.log,
            http: config.http,
            cl_mempool: nomos_mempool::Settings {
                backend: (),
                network: AdapterSettings {
                    topic: String::from(nomos_node::CL_TOPIC),
                    id: <Tx as Transaction>::hash,
                },
            },
            da_mempool: nomos_mempool::Settings {
                backend: (),
                network: AdapterSettings {
                    topic: String::from(nomos_node::DA_TOPIC),
                    id: cert_id,
                },
            },
            consensus: config.consensus,
            bridges: HttpBridgeSettings { bridges },
            #[cfg(feature = "metrics")]
            metrics: config.metrics,
            da: config.da,
            storage: nomos_storage::backends::sled::SledBackendSettings {
                db_path: std::path::PathBuf::from(DEFAULT_DB_PATH),
            },
        },
        None,
    )
    .map_err(|e| eyre!("Error encountered: {}", e))?;
    app.wait_finished();
    Ok(())
}

fn cert_id(cert: &Certificate) -> <Blob as blob::Blob>::Hash {
    use certificate::Certificate;
    cert.hash()
}
