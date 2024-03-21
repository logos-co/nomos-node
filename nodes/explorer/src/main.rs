use clap::Parser;
use eyre::Result;

use nomos_explorer::{ApiArgs, Config, Explorer};
use nomos_node::LogArgs;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path for a yaml-encoded network config file
    config: std::path::PathBuf,
    /// Overrides log config.
    #[clap(flatten)]
    log_args: LogArgs,
    /// Overrides network config.
    /// Overrides http config.
    #[clap(flatten)]
    api_args: ApiArgs,
}

fn main() -> Result<()> {
    let Args {
        config,
        log_args,
        api_args,
    } = Args::parse();
    let config: Config = serde_yaml::from_reader::<_, Config>(std::fs::File::open(config)?)?
        .update_api(api_args)?
        .update_log(log_args)?;

    Explorer::run(config)
}
