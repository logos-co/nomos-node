use crate::api_backend::AxumBackend;

use eyre::{eyre, Result};
use nomos_api::ApiService;
use nomos_log::{Logger, LoggerBackend, LoggerFormat};
use nomos_node::config::LoggerBackendType;
use nomos_node::{HttpArgs, LogArgs, Tx, Wire};
use nomos_storage::backends::rocksdb::RocksBackend;
use nomos_storage::StorageService;
use overwatch_rs::services::ServiceData;
use serde::{Deserialize, Serialize};
use tracing::Level;

pub type ApiArgs = HttpArgs;

#[derive(Deserialize, Debug, Clone, Serialize)]
pub struct Config {
    pub log: <Logger as ServiceData>::Settings,
    pub api: <ApiService<AxumBackend<Tx, Wire>> as ServiceData>::Settings,
    pub storage: <StorageService<RocksBackend<Wire>> as ServiceData>::Settings,
}

impl Config {
    pub fn update_log(mut self, log_args: LogArgs) -> Result<Self> {
        let LogArgs {
            backend,
            log_addr: addr,
            directory,
            prefix,
            format,
            level,
        } = log_args;

        // Override the file config with the one from env variables.
        if let Some(backend) = backend {
            self.log.backend = match backend {
                LoggerBackendType::Gelf => LoggerBackend::Gelf {
                    addr: addr.ok_or_else(|| eyre!("Gelf backend requires an address."))?,
                },
                LoggerBackendType::File => LoggerBackend::File {
                    directory: directory
                        .ok_or_else(|| eyre!("File backend requires a directory."))?,
                    prefix,
                },
                LoggerBackendType::Stdout => LoggerBackend::Stdout,
                LoggerBackendType::Stderr => LoggerBackend::Stderr,
            }
        };

        // Update parts of the config.
        if let Some(format_str) = format {
            self.log.format = match format_str.as_str() {
                "Json" => LoggerFormat::Json,
                "Plain" => LoggerFormat::Plain,
                _ => return Err(eyre!("Invalid log format provided.")),
            };
        }
        if let Some(level_str) = level {
            self.log.level = match level_str.as_str() {
                "DEBUG" => Level::DEBUG,
                _ => return Err(eyre!("Invalid log level provided.")),
            };
        }
        Ok(self)
    }
    pub fn update_api(mut self, http_args: ApiArgs) -> Result<Self> {
        let ApiArgs {
            http_addr,
            cors_origins,
        } = http_args;

        if let Some(addr) = http_addr {
            self.api.backend_settings.address = addr;
        }

        if let Some(cors) = cors_origins {
            self.api.backend_settings.cors_origins = cors;
        }

        Ok(self)
    }
}
