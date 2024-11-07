use reqwest::Url;

pub mod config;
pub mod repo;

#[derive(Clone)]
pub struct TracingParams {
    pub tempo_endpoint: Url,
    pub loki_endpoint: Url,
    pub metrics_endpoint: Url,
}
