use kzgrs_backend::dispersal::BlobInfo;
use nomos_node::{
    config::MixArgs, Config, CryptarchiaArgs, HttpArgs, LogArgs, NetworkArgs, Nomos,
    NomosServiceSettings, Tx,
};

use clap::Parser;
use color_eyre::eyre::{eyre, Result};
use nomos_core::{da::blob::info::DispersedBlobInfo, tx::Transaction};

use nomos_mempool::network::adapters::libp2p::Settings as AdapterSettings;

use overwatch_rs::overwatch::*;
use tracing::{span, Level};
use uuid::Uuid;

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
    /// Overrides mix config.
    #[clap(flatten)]
    mix_args: MixArgs,
    /// Overrides http config.
    #[clap(flatten)]
    http_args: HttpArgs,
    #[clap(flatten)]
    cryptarchia_args: CryptarchiaArgs,
}

fn main() -> Result<()> {
    let Args {
        config,
        log_args,
        http_args,
        network_args,
        mix_args,
        cryptarchia_args,
    } = Args::parse();
    let config = serde_yaml::from_reader::<_, Config>(std::fs::File::open(config)?)?
        .update_from_args(
            log_args,
            network_args,
            mix_args,
            http_args,
            cryptarchia_args,
        )?;

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
            mix: config.mix,
            #[cfg(feature = "tracing")]
            tracing: config.tracing,
            http: config.http,
            cl_mempool: nomos_mempool::TxMempoolSettings {
                backend: (),
                network: AdapterSettings {
                    topic: String::from(nomos_node::CL_TOPIC),
                    id: <Tx as Transaction>::hash,
                },
            },
            da_mempool: nomos_mempool::DaMempoolSettings {
                backend: (),
                network: AdapterSettings {
                    topic: String::from(nomos_node::DA_TOPIC),
                    id: <BlobInfo as DispersedBlobInfo>::blob_id,
                },
            },
            da_network: config.da_network,
            da_indexer: config.da_indexer,
            da_sampling: config.da_sampling,
            da_verifier: config.da_verifier,
            cryptarchia: config.cryptarchia,
            storage: config.storage,
            system_sig: (),
        },
        None,
    )
    .map_err(|e| eyre!("Error encountered: {}", e))?;
    app.wait_finished();
    Ok(())
}
