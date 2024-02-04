mod api_backend;
mod config;

use clap::Parser;
use eyre::{eyre, Result};
use nomos_api::ApiService;
use overwatch_rs::overwatch::OverwatchRunner;
use overwatch_rs::services::handle::ServiceHandle;
use overwatch_rs::Services;

use crate::api_backend::AxumBackend;
use crate::config::{ApiArgs, Config};
use nomos_log::Logger;
use nomos_node::{DataAvailability, LogArgs, Tx, Wire};
use nomos_storage::backends::sled::SledBackendSettings;
use nomos_storage::{backends::sled::SledBackend, StorageService};

#[derive(Services)]
struct Explorer {
    log: ServiceHandle<Logger>,
    storage: ServiceHandle<StorageService<SledBackend<Wire>>>,
    da: ServiceHandle<DataAvailability>,
    api: ServiceHandle<ApiService<AxumBackend<Tx, Wire>>>,
}

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
    /// Overrides http config.
    #[clap(flatten)]
    api_args: ApiArgs,
}

fn main() -> Result<()> {
    let Args {
        config,
        log_args,
        api_args,
    } = Args::parse();
    let config: Config = serde_yaml::from_reader::<_, Config>(std::fs::File::open(config)?)?
        .update_api(api_args)?
        .update_log(log_args)?;

    let app = OverwatchRunner::<Explorer>::run(
        ExplorerServiceSettings {
            log: config.log,
            storage: SledBackendSettings {
                db_path: DEFAULT_DB_PATH.into(),
            },
            da: config.da,
            api: config.api,
        },
        None,
    )
    .map_err(|e| eyre!("Error encountered: {}", e))?;
    app.wait_finished();
    Ok(())
}
