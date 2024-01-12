use full_replication::{Blob, Certificate};
use nomos_metrics::{MetricsSettings, NomosRegistry};
use nomos_node::{
    Config, ConsensusArgs, DaArgs, HttpArgs, LogArgs, MetricsArgs, NetworkArgs, Nomos,
    NomosServiceSettings, OverlayArgs, Tx,
};

use clap::Parser;
use color_eyre::eyre::{eyre, Result};
use nomos_core::{
    da::{blob, certificate},
    tx::Transaction,
};

use nomos_mempool::network::adapters::libp2p::Settings as AdapterSettings;

use overwatch_rs::overwatch::*;

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
    /// Overrides da config.
    #[clap(flatten)]
    da_args: DaArgs,
    /// Overrides metrics config.
    #[clap(flatten)]
    metrics_args: MetricsArgs,
}

fn main() -> Result<()> {
    let Args {
        config,
        da_args,
        log_args,
        http_args,
        network_args,
        consensus_args,
        overlay_args,
        metrics_args,
    } = Args::parse();
    let config = serde_yaml::from_reader::<_, Config>(std::fs::File::open(config)?)?
        .update_da(da_args)?
        .update_log(log_args)?
        .update_http(http_args)?
        .update_consensus(consensus_args)?
        .update_overlay(overlay_args)?
        .update_network(network_args)?;

    let metrics = metrics_args.with_metrics.then(NomosRegistry::default);

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
                metrics: metrics.clone(),
            },
            da_mempool: nomos_mempool::Settings {
                backend: (),
                network: AdapterSettings {
                    topic: String::from(nomos_node::DA_TOPIC),
                    id: cert_id,
                },
                metrics: metrics.clone(),
            },
            consensus: config.consensus,
            metrics: MetricsSettings { registry: metrics },
            da: config.da,
            storage: nomos_storage::backends::sled::SledBackendSettings {
                db_path: std::path::PathBuf::from(DEFAULT_DB_PATH),
            },
            system_sig: (),
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
