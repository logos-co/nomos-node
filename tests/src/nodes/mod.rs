pub mod nomos;
pub use nomos::NomosNode;

use nomos_da_network_service::{
    backends::libp2p::executor::DaNetworkExecutorBackendSettings, NetworkConfig,
};
use nomos_executor::{api::backend::AxumBackendSettings, config::Config as ExecutorConfig};
use nomos_node::Config as NodeConfig;

use overwatch_rs::services::ServiceData;
use tempfile::TempDir;

const LOGS_PREFIX: &str = "__logs";

fn create_tempdir() -> std::io::Result<TempDir> {
    // It's easier to use the current location instead of OS-default tempfile location
    // because Github Actions can easily access files in the current location using wildcard
    // to upload them as artifacts.
    tempfile::TempDir::new_in(std::env::current_dir()?)
}

fn persist_tempdir(tempdir: &mut TempDir, label: &str) -> std::io::Result<()> {
    println!(
        "{}: persisting directory at {}",
        label,
        tempdir.path().display()
    );
    // we need ownership of the dir to persist it
    let dir = std::mem::replace(tempdir, tempfile::tempdir()?);
    // a bit confusing but `into_path` persists the directory
    let _ = dir.into_path();
    Ok(())
}

pub struct ExecutorSettings {
    pub da_dispersal: <nomos_executor::DaDispersal as ServiceData>::Settings,
    pub num_subnets: u16,
}

pub fn executor_config_from_node_config(
    nomos_config: NodeConfig,
    executor_settings: ExecutorSettings,
) -> ExecutorConfig {
    ExecutorConfig {
        log: nomos_config.log,
        network: nomos_config.network,
        da_dispersal: executor_settings.da_dispersal,
        da_network: NetworkConfig {
            backend: DaNetworkExecutorBackendSettings {
                validator_settings: nomos_config.da_network.backend,
                num_subnets: executor_settings.num_subnets,
            },
        },
        da_indexer: nomos_config.da_indexer,
        da_verifier: nomos_config.da_verifier,
        da_sampling: nomos_config.da_sampling,
        http: nomos_api::ApiServiceSettings {
            backend_settings: AxumBackendSettings {
                address: nomos_config.http.backend_settings.address,
                cors_origins: nomos_config.http.backend_settings.cors_origins,
            },
        },
        cryptarchia: nomos_config.cryptarchia,
        storage: nomos_config.storage,
    }
}
