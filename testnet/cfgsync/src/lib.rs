use reqwest::Url;
use std::str::FromStr;
use tracing::Level;

pub mod config;
pub mod repo;

#[derive(Clone)]
pub enum LogOutput {
    Stdout,
    Loki,
}

#[derive(Debug)]
pub struct ParseLogOutputError;

impl FromStr for LogOutput {
    type Err = ParseLogOutputError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "stdout" => Ok(LogOutput::Stdout),
            "loki" => Ok(LogOutput::Loki),
            _ => Err(ParseLogOutputError),
        }
    }
}

#[derive(Clone)]
pub struct TracingParams {
    pub tempo_endpoint: Url,
    pub loki_endpoint: Url,
    pub metrics_endpoint: Url,
    pub log_output: LogOutput,
    pub log_level: Level,
}
