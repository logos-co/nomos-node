use clap::Parser;
use color_eyre::eyre::Result;
use nomos_log::Logger;
use nomos_network::{backends::Waku, NetworkService};
use overwatch_derive::*;
use overwatch_rs::{
    overwatch::*,
    services::{handle::ServiceHandle, ServiceData},
};
use serde::Deserialize;

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
}

#[derive(Services)]
struct Services {
    logging: ServiceHandle<Logger>,
    network: ServiceHandle<NetworkService<Waku>>,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let Args { config } = Args::parse();
    let config = serde_yaml::from_reader::<_, Config>(std::fs::File::open(config)?)?;
    let app = OverwatchRunner::<Services>::run(
        ServicesServiceSettings {
            network: config.network,
            logging: config.log,
        },
        None,
    );
    app.wait_finished();
    Ok(())
}
