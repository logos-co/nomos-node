use reqwest::Url;
use tracing::{log, Level};

pub mod config;
pub mod repo;

#[derive(Clone)]
pub enum LogOutput {
    Stdout,
    Loki,
}

#[derive(Debug)]
pub struct ParseLogOutputError;

impl TryFrom<&str> for LogOutput {
    type Error = ParseLogOutputError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "Stdout" => Ok(LogOutput::Stdout),
            "Loki" => Ok(LogOutput::Loki),
            _ => Err(ParseLogOutputError),
        }
    }
}

#[derive(Debug)]
pub struct ParseLevelError;

pub trait LevelExt {
    fn from_str(s: &str) -> Result<Self, ParseLevelError>
    where
        Self: Sized;
}

impl LevelExt for Level {
    fn from_str(s: &str) -> Result<Self, ParseLevelError> {
        match s.to_ascii_lowercase().as_str() {
            "trace" => Ok(Level::TRACE),
            "debug" => Ok(Level::DEBUG),
            "info" => Ok(Level::INFO),
            "warn" => Ok(Level::WARN),
            "error" => Ok(Level::ERROR),
            _ => Err(ParseLevelError),
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
