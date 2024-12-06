use std::net::ToSocketAddrs;
use std::path::PathBuf;
use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use color_eyre::eyre::{eyre, Result};
use tracing::Level;

use nomos_tracing_service::{LoggerLayer, Tracing};
use overwatch_rs::services::ServiceData;
use nomos_tracing::logging::gelf::GelfConfig;
use nomos_tracing::logging::local::FileConfig;

#[derive(ValueEnum, Clone, Debug, Default)]
pub enum LoggerLayerType {
    Gelf,
    File,
    #[default]
    Stdout,
    Stderr,
}


#[derive(Parser, Debug, Clone)]
pub struct LogArgs {
    /// Address for the Gelf backend
    #[clap(long = "log-addr", env = "LOG_ADDR", required_if_eq("backend", "Gelf"))]
    log_addr: Option<String>,

    /// Directory for the File backend
    #[clap(long = "log-dir", env = "LOG_DIR", required_if_eq("backend", "File"))]
    directory: Option<PathBuf>,

    /// Prefix for the File backend
    #[clap(long = "log-path", env = "LOG_PATH", required_if_eq("backend", "File"))]
    prefix: Option<PathBuf>,

    /// Backend type
    #[clap(long = "log-backend", env = "LOG_BACKEND", value_enum)]
    backend: Option<LoggerLayerType>,

    #[clap(long = "log-level", env = "LOG_LEVEL")]
    level: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Serialize)]
pub struct Config {
    pub tracing: <Tracing as ServiceData>::Settings,
}

impl Config {
    pub fn update_from_args(
        mut self,
        log_args: LogArgs,
    ) -> Result<Self> {
        update_tracing(&mut self.tracing, log_args)?;
        Ok(self)
    }
}

pub fn update_tracing(
    tracing: &mut <Tracing as ServiceData>::Settings,
    tracing_args: LogArgs,
) -> Result<()> {
    let LogArgs {
        backend,
        log_addr: addr,
        directory,
        prefix,
        level,
    } = tracing_args;

    // Override the file config with the one from env variables.
    if let Some(backend) = backend {
        tracing.logger = match backend {
            LoggerLayerType::Gelf => LoggerLayer::Gelf(GelfConfig {
                addr: addr
                    .ok_or_else(|| eyre!("Gelf backend requires an address."))?
                    .to_socket_addrs()?
                    .next()
                    .ok_or_else(|| eyre!("Invalid gelf address"))?,
            }),
            LoggerLayerType::File => LoggerLayer::File(FileConfig {
                directory: directory.ok_or_else(|| eyre!("File backend requires a directory."))?,
                prefix,
            }),
            LoggerLayerType::Stdout => LoggerLayer::Stdout,
            LoggerLayerType::Stderr => LoggerLayer::Stderr,
        }
    };

    if let Some(level_str) = level {
        tracing.level = match level_str.as_str() {
            "DEBUG" => Level::DEBUG,
            "INFO" => Level::INFO,
            "ERROR" => Level::ERROR,
            "WARN" => Level::WARN,
            _ => return Err(eyre!("Invalid log level provided.")),
        };
    }
    Ok(())
}