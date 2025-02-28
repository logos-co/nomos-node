use clap::Parser;
use color_eyre::eyre::{eyre, Result};
use nomos_executor::{
    config::Config as ExecutorConfig, NomosExecutor, NomosExecutorServiceSettings,
};
use nomos_mempool::tx::settings::TxMempoolSettings;
use nomos_node::{
    config::BlendArgs, BlobInfo, CryptarchiaArgs, DaMempoolSettings, DispersedBlobInfo, HttpArgs,
    LogArgs, MempoolAdapterSettings, NetworkArgs, Transaction, Tx, CL_TOPIC, DA_TOPIC,
};
use overwatch_rs::overwatch::*;

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
    /// Overrides blend config.
    #[clap(flatten)]
    blend_args: BlendArgs,
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
        blend_args,
        cryptarchia_args,
    } = Args::parse();
    let config = serde_yaml::from_reader::<_, ExecutorConfig>(std::fs::File::open(config)?)?
        .update_from_args(
            log_args,
            network_args,
            blend_args,
            http_args,
            cryptarchia_args,
        )?;

    let app = OverwatchRunner::<NomosExecutor>::run(
        NomosExecutorServiceSettings {
            network: config.network,
            blend: config.blend,
            #[cfg(feature = "tracing")]
            tracing: config.tracing,
            http: config.http,
            cl_mempool: TxMempoolSettings {
                pool: (),
                network_adapter: MempoolAdapterSettings {
                    topic: String::from(CL_TOPIC),
                    id: <Tx as Transaction>::hash,
                },
                recovery_path: config.mempool.recovery_path,
            },
            da_mempool: DaMempoolSettings {
                backend: (),
                network: MempoolAdapterSettings {
                    topic: String::from(DA_TOPIC),
                    id: <BlobInfo as DispersedBlobInfo>::blob_id,
                },
            },
            da_dispersal: config.da_dispersal,
            da_network: config.da_network,
            da_indexer: config.da_indexer,
            da_sampling: config.da_sampling,
            da_verifier: config.da_verifier,
            cryptarchia: config.cryptarchia,
            time: config.time,
            storage: config.storage,
            system_sig: (),
        },
        None,
    )
    .map_err(|e| eyre!("Error encountered: {}", e))?;
    app.wait_finished();
    Ok(())
}
