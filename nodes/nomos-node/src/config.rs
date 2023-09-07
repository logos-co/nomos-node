use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
};

use crate::Carnot;
use clap::{Parser, ValueEnum};
use color_eyre::eyre::{eyre, Result};
#[cfg(feature = "metrics")]
use metrics::{backend::map::MapMetricsBackend, types::MetricsData, MetricsService};
use nomos_http::{backends::axum::AxumBackend, http::HttpService};
#[cfg(feature = "libp2p")]
use nomos_libp2p::{secp256k1::SecretKey, Multiaddr};
use nomos_log::{Logger, LoggerBackend, LoggerFormat};
#[cfg(feature = "libp2p")]
use nomos_network::backends::libp2p::Libp2p;
#[cfg(feature = "waku")]
use nomos_network::backends::waku::Waku;
use nomos_network::NetworkService;
use overwatch_rs::services::ServiceData;
use serde::{Deserialize, Serialize};
use tracing::Level;
#[cfg(feature = "waku")]
use waku_bindings::{Multiaddr, SecretKey};

#[derive(ValueEnum, Clone, Debug, Default)]
pub enum LoggerBackendType {
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
    log_addr: Option<SocketAddr>,

    /// Directory for the File backend
    #[clap(long = "log-dir", env = "LOG_DIR", required_if_eq("backend", "File"))]
    directory: Option<PathBuf>,

    /// Prefix for the File backend
    #[clap(long = "log-path", env = "LOG_PATH", required_if_eq("backend", "File"))]
    prefix: Option<PathBuf>,

    /// Backend type
    #[clap(long = "log-backend", env = "LOG_BACKEND", value_enum)]
    backend: Option<LoggerBackendType>,

    #[clap(long = "log-format", env = "LOG_FORMAT")]
    format: Option<String>,

    #[clap(long = "log-level", env = "LOG_LEVEL")]
    level: Option<String>,
}

#[derive(Parser, Debug, Clone)]
pub struct NetworkArgs {
    #[clap(long = "net-host", env = "NET_HOST")]
    host: Option<IpAddr>,

    #[clap(long = "net-port", env = "NET_PORT")]
    port: Option<usize>,

    #[clap(long = "net-node-key", env = "NET_NODE_KEY")]
    node_key: Option<String>,

    #[clap(long = "net-initial-peers", env = "NET_INITIAL_PEERS")]
    pub initial_peers: Option<Vec<Multiaddr>>,
}

#[derive(Parser, Debug, Clone)]
pub struct HttpArgs {
    #[clap(long = "http-host", env = "HTTP_HOST")]
    http_addr: Option<SocketAddr>,

    #[clap(long = "http-cors-origin", env = "HTTP_CORS_ORIGIN")]
    pub cors_origins: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, Clone, Serialize)]
pub struct Config {
    pub log: <Logger as ServiceData>::Settings,
    #[cfg(feature = "waku")]
    pub network: <NetworkService<Waku> as ServiceData>::Settings,
    #[cfg(feature = "libp2p")]
    pub network: <NetworkService<Libp2p> as ServiceData>::Settings,
    pub http: <HttpService<AxumBackend> as ServiceData>::Settings,
    pub consensus: <Carnot as ServiceData>::Settings,
    #[cfg(feature = "metrics")]
    pub metrics: <MetricsService<MapMetricsBackend<MetricsData>> as ServiceData>::Settings,
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

    #[cfg(feature = "waku")]
    pub fn update_network(mut self, network_args: NetworkArgs) -> Result<Self> {
        let NetworkArgs {
            host,
            port,
            node_key,
            initial_peers,
        } = network_args;

        if let Some(host) = host {
            self.network.backend.inner.host = Some(host);
        }

        if let Some(port) = port {
            self.network.backend.inner.port = Some(port);
        }

        if let Some(node_key) = node_key {
            use std::str::FromStr;
            self.network.backend.inner.node_key = Some(SecretKey::from_str(&node_key)?);
        }

        if let Some(peers) = initial_peers {
            self.network.backend.initial_peers = peers;
        }

        Ok(self)
    }

    #[cfg(feature = "libp2p")]
    pub fn update_network(mut self, network_args: NetworkArgs) -> Result<Self> {
        let NetworkArgs {
            host,
            port,
            node_key,
            initial_peers,
        } = network_args;

        if let Some(IpAddr::V4(h)) = host {
            self.network.backend.host = h;
        } else if host.is_some() {
            return Err(eyre!("Unsupported ip version"));
        }

        if let Some(port) = port {
            self.network.backend.port = port as u16;
        }

        if let Some(node_key) = node_key {
            let mut key_bytes = hex::decode(node_key)?;
            self.network.backend.node_key = SecretKey::try_from_bytes(key_bytes.as_mut_slice())?;
        }

        if let Some(peers) = initial_peers {
            self.network.backend.initial_peers = peers;
        }

        Ok(self)
    }

    pub fn update_http(mut self, http_args: HttpArgs) -> Result<Self> {
        let HttpArgs {
            http_addr: addr,
            cors_origins,
        } = http_args;

        if let Some(addr) = addr {
            self.http.backend.address = addr;
        }

        if let Some(cors) = cors_origins {
            self.http.backend.cors_origins = cors;
        }

        Ok(self)
    }
}
