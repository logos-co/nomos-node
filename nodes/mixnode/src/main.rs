mod services;

use clap::Parser;
use color_eyre::eyre::Result;
use mixnode::{Config, MixNode, MixNodeServiceSettings};
use overwatch_rs::overwatch::OverwatchRunner;
use overwatch_rs::DynError;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path for a yaml-encoded mixnet-node config file
    config: std::path::PathBuf,
}

fn main() -> Result<(), DynError> {
    // Construct a subscriber that prints formatted traces to stdout
    // and use that subscriber to process traces emitted after this point
    // TODO: use the log service that nomos-node uses, if necessary
    let subscriber = tracing_subscriber::FmtSubscriber::new();
    tracing::subscriber::set_global_default(subscriber)?;

    let Args { config } = Args::parse();
    let config = serde_yaml::from_reader::<_, Config>(std::fs::File::open(config)?)?;

    let app = OverwatchRunner::<MixNode>::run(
        MixNodeServiceSettings {
            node: config.mixnode,
        },
        None,
    )?;
    app.wait_finished();
    Ok(())
}
