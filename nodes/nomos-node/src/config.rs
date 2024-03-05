use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    time::Duration,
};

use crate::api::AxumBackend;
use crate::DataAvailability;
use crate::{Carnot, Tx, Wire, MB16};
use clap::{Parser, ValueEnum};
use color_eyre::eyre::{self, eyre, Result};
use hex::FromHex;
use nomos_api::ApiService;
use nomos_libp2p::{secp256k1::SecretKey, Multiaddr};
use nomos_log::{Logger, LoggerBackend, LoggerFormat};
use nomos_mempool::verifier::certificate::mock::MockDaVerifierSettings;
use nomos_network::backends::libp2p::Libp2p;
use nomos_network::NetworkService;
use overwatch_rs::services::ServiceData;
use serde::{Deserialize, Serialize};
use tracing::Level;

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

    #[clap(long = "net-initial-peers", env = "NET_INITIAL_PEERS", num_args = 1.., value_delimiter = ',')]
    pub initial_peers: Option<Vec<Multiaddr>>,
}

#[derive(Parser, Debug, Clone)]
pub struct HttpArgs {
    #[clap(long = "http-host", env = "HTTP_HOST")]
    http_addr: Option<SocketAddr>,

    #[clap(long = "http-cors-origin", env = "HTTP_CORS_ORIGIN")]
    pub cors_origins: Option<Vec<String>>,
}

#[derive(Parser, Debug, Clone)]
pub struct ConsensusArgs {
    #[clap(long = "consensus-priv-key", env = "CONSENSUS_PRIV_KEY")]
    consensus_priv_key: Option<String>,

    #[clap(long = "consensus-timeout-secs", env = "CONSENSUS_TIMEOUT_SECS")]
    consensus_timeout_secs: Option<String>,
}

#[derive(Parser, Debug, Clone)]
pub struct OverlayArgs {
    #[clap(long = "overlay-nodes", env = "OVERLAY_NODES", num_args = 1.., value_delimiter = ',')]
    pub overlay_nodes: Option<Vec<String>>,

    #[clap(long = "overlay-leader", env = "OVERLAY_LEADER")]
    pub overlay_leader: Option<String>,

    #[clap(
        long = "overlay-number-of-committees",
        env = "OVERLAY_NUMBER_OF_COMMITTEES"
    )]
    pub overlay_number_of_committees: Option<usize>,

    #[clap(
        long = "overlay-super-majority-threshold",
        env = "OVERLAY_SUPER_MAJORITY_THRESHOLD"
    )]
    pub overlay_super_majority_threshold: Option<f32>,
}

#[derive(Parser, Debug, Clone)]
pub struct DaArgs {
    #[clap(long = "da-voter", env = "DA_VOTER")]
    da_voter: Option<String>,
}

#[derive(Parser, Debug, Clone)]
pub struct MetricsArgs {
    #[clap(long = "with-metrics", env = "WITH_METRICS")]
    pub with_metrics: bool,
}

#[derive(Deserialize, Debug, Clone, Serialize)]
pub struct Config {
    pub log: <Logger as ServiceData>::Settings,
    pub network: <NetworkService<Libp2p> as ServiceData>::Settings,
    pub http: <ApiService<AxumBackend<Tx, Wire, MB16>> as ServiceData>::Settings,
    pub consensus: <Carnot as ServiceData>::Settings,
    pub da: <DataAvailability as ServiceData>::Settings,
    pub da_verifier: MockDaVerifierSettings,
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
    pub fn update_network(mut self, network_args: NetworkArgs) -> Result<Self> {
        let NetworkArgs {
            host,
            port,
            node_key,
            initial_peers,
        } = network_args;

        if let Some(IpAddr::V4(h)) = host {
            self.network.backend.inner.host = h;
        } else if host.is_some() {
            return Err(eyre!("Unsupported ip version"));
        }

        if let Some(port) = port {
            self.network.backend.inner.port = port as u16;
        }

        if let Some(node_key) = node_key {
            let mut key_bytes = hex::decode(node_key)?;
            self.network.backend.inner.node_key =
                SecretKey::try_from_bytes(key_bytes.as_mut_slice())?;
        }

        if let Some(peers) = initial_peers {
            self.network.backend.initial_peers = peers;
        }

        Ok(self)
    }

    pub fn update_http(mut self, http_args: HttpArgs) -> Result<Self> {
        let HttpArgs {
            http_addr,
            cors_origins,
        } = http_args;

        if let Some(addr) = http_addr {
            self.http.backend_settings.address = addr;
        }

        if let Some(cors) = cors_origins {
            self.http.backend_settings.cors_origins = cors;
        }

        Ok(self)
    }

    pub fn update_consensus(mut self, consensus_args: ConsensusArgs) -> Result<Self> {
        let ConsensusArgs {
            consensus_priv_key,
            consensus_timeout_secs,
        } = consensus_args;

        if let Some(private_key) = consensus_priv_key {
            let bytes = <[u8; 32]>::from_hex(private_key)?;
            self.consensus.private_key = bytes;
        }

        if let Some(timeout) = consensus_timeout_secs {
            let secs = timeout.parse::<u64>()?;
            self.consensus.timeout = Duration::from_secs(secs);
        }

        Ok(self)
    }

    pub fn update_overlay(mut self, overlay_args: OverlayArgs) -> Result<Self> {
        let OverlayArgs {
            overlay_nodes,
            overlay_leader,
            overlay_number_of_committees,
            overlay_super_majority_threshold,
        } = overlay_args;

        if let Some(nodes) = overlay_nodes {
            self.consensus.overlay_settings.nodes = nodes
                .iter()
                .map(|n| {
                    <[u8; 32]>::from_hex(n)
                        .map_err(|e| eyre::eyre!("Failed to decode hex: {}", e))
                        .map(|b| b.into())
                })
                .collect::<Result<Vec<_>, eyre::Report>>()?;
        }

        if let Some(leader) = overlay_leader {
            let bytes = <[u8; 32]>::from_hex(leader)?;
            self.consensus.overlay_settings.current_leader = bytes.into();
        }

        if let Some(committees) = overlay_number_of_committees {
            self.consensus.overlay_settings.number_of_committees = committees;
        }

        if let Some(super_majority_threshold) = overlay_super_majority_threshold {
            self.consensus.overlay_settings.super_majority_threshold =
                Some(super_majority_threshold.into());
        }

        Ok(self)
    }

    pub fn update_da(mut self, da_args: DaArgs) -> Result<Self> {
        let DaArgs { da_voter } = da_args;

        if let Some(voter) = da_voter {
            let bytes = <[u8; 32]>::from_hex(voter)?;
            self.da.da_protocol.voter = bytes;
        }

        Ok(self)
    }
}
