use nomos_node::*;
#[cfg(feature = "metrics")]
use nomos_node::{MetricsSettings, NomosRegistry};

use clap::Parser;
use color_eyre::eyre::{eyre, Result};
use overwatch_rs::overwatch::*;
use tracing::{span, Level};
use uuid::Uuid;

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
    #[clap(flatten)]
    cryptarchia_args: CryptarchiaArgs,
    /// Overrides metrics config.
    #[clap(flatten)]
    metrics_args: MetricsArgs,
}

fn main() -> Result<()> {
    let Args {
        config,
        log_args,
        http_args,
        network_args,
        cryptarchia_args,
        metrics_args,
    } = Args::parse();
    let config = serde_yaml::from_reader::<_, Config>(std::fs::File::open(config)?)?
        .update_log(log_args)?
        .update_http(http_args)?
        .update_network(network_args)?
        .update_cryptarchia_consensus(cryptarchia_args)?;

    let registry = cfg!(feature = "metrics")
        .then(|| metrics_args.with_metrics.then(NomosRegistry::default))
        .flatten();

    #[cfg(debug_assertions)]
    let debug_span = {
        let debug_id = Uuid::new_v4();
        span!(Level::DEBUG, "Nomos", debug_id = debug_id.to_string())
    };
    #[cfg(debug_assertions)]
    let _guard = debug_span.enter();
    let app = OverwatchRunner::<Nomos>::run(
        NomosServiceSettings {
            network: config.network,
            #[cfg(feature = "tracing")]
            logging: config.log,
            http: config.http,
            cl_mempool: TxMempoolSettings {
                backend: (),
                network: MempoolAdapterSettings {
                    topic: String::from(CL_TOPIC),
                    id: <Tx as Transaction>::hash,
                },
                registry: registry.clone(),
            },
            da_mempool: DaMempoolSettings {
                backend: (),
                network: MempoolAdapterSettings {
                    topic: String::from(DA_TOPIC),
                    id: <BlobInfo as DispersedBlobInfo>::blob_id,
                },
                registry: registry.clone(),
            },
            da_network: config.da_network,
            da_indexer: config.da_indexer,
            da_sampling: config.da_sampling,
            da_verifier: config.da_verifier,
            cryptarchia: config.cryptarchia,
            #[cfg(feature = "metrics")]
            metrics: MetricsSettings { registry },
            storage: RocksBackendSettings {
                db_path: std::path::PathBuf::from(DEFAULT_DB_PATH),
                read_only: false,
                column_family: Some("blocks".into()),
            },
            system_sig: (),
        },
        None,
    )
    .map_err(|e| eyre!("Error encountered: {}", e))?;
    app.wait_finished();
    Ok(())
}
