use clap::Parser;
use color_eyre::eyre::{eyre, Result};
use nomos_log::Logger;
use nomos_network::{backends::waku::Waku, NetworkService};
use overwatch_derive::*;
use overwatch_rs::{
    overwatch::*,
    services::{handle::ServiceHandle, ServiceData},
};
use serde::Deserialize;

#[cfg(feature = "metrics")]
use metrics::{backend::map::MapMetricsBackend, types::MetricsData, MetricsService};

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path for a yaml-encoded network config file
    config: std::path::PathBuf,
}

#[derive(Deserialize)]
struct Config {
    log: <Logger as ServiceData>::Settings,
    network: <NetworkService<Waku> as ServiceData>::Settings,
    #[cfg(feature = "metrics")]
    metrics: <MetricsService<MapMetricsBackend<MetricsData>> as ServiceData>::Settings,
}

#[derive(Services)]
struct Nomos {
    logging: ServiceHandle<Logger>,
    network: ServiceHandle<NetworkService<Waku>>,
    #[cfg(feature = "metrics")]
    metrics: ServiceHandle<MetricsService<MapMetricsBackend<MetricsData>>>,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let Args { config } = Args::parse();
    let config = serde_yaml::from_reader::<_, Config>(std::fs::File::open(config)?)?;
    let app = OverwatchRunner::<Nomos>::run(
        NomosServiceSettings {
            network: config.network,
            logging: config.log,
            #[cfg(feature = "metrics")]
            metrics: config.metrics,
        },
        None,
    )
    .map_err(|e| eyre!("Error encountered: {}", e))?;
    app.wait_finished();
    Ok(())
}
