pub use crate::api_backend::{AxumBackend, AxumBackendSettings};
pub use crate::config::{ApiArgs, Config};

use eyre::{eyre, Result};

use overwatch_rs::overwatch::OverwatchRunner;
use overwatch_rs::services::handle::ServiceHandle;
use overwatch_rs::Services;

use nomos_api::ApiService;
use nomos_log::Logger;
use nomos_node::{Tx, Wire};
use nomos_storage::{backends::rocksdb::RocksBackend, StorageService};

mod api_backend;
mod config;

#[derive(Services)]
pub struct Explorer {
    pub log: ServiceHandle<Logger>,
    pub storage: ServiceHandle<StorageService<RocksBackend<Wire>>>,
    pub api: ServiceHandle<ApiService<AxumBackend<Tx, Wire>>>,
}

impl Explorer {
    pub fn run(config: Config) -> Result<()> {
        let app = OverwatchRunner::<Explorer>::run(
            ExplorerServiceSettings {
                log: config.log,
                storage: config.storage,
                api: config.api,
            },
            None,
        )
        .map_err(|e| eyre!("Error encountered: {}", e))?;
        app.wait_finished();
        Ok(())
    }
}
