use clap::Parser;
use nomos_cli::Cli;
use nomos_tracing::logging::local::create_writer_layer;
use tracing::{level_filters::LevelFilter, Level};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() {
    let (logger_layer, _guard) = create_writer_layer(std::io::stdout());
    tracing_subscriber::registry()
        .with(LevelFilter::from(Level::DEBUG))
        .with(logger_layer)
        .init();

    let cli = Cli::parse();
    cli.run().unwrap();
}
