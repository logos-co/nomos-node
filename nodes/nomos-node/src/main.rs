use clap::Parser;
use color_eyre::eyre::Result;
use nomos_node::{
    Config, ConsensusArgs, DaArgs, HttpArgs, LogArgs, MetricsArgs, NetworkArgs, OverlayArgs,
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path for a yaml-encoded network config file
    config: std::path::PathBuf,
    /// Overrides log config.
    #[clap(flatten)]
    log_args: LogArgs,
    /// Overrides network config.
    #[clap(flatten)]
    network_args: NetworkArgs,
    /// Overrides http config.
    #[clap(flatten)]
    http_args: HttpArgs,
    /// Overrides consensus config.
    #[clap(flatten)]
    consensus_args: ConsensusArgs,
    /// Overrides overlay config.
    #[clap(flatten)]
    overlay_args: OverlayArgs,
    /// Overrides da config.
    #[clap(flatten)]
    da_args: DaArgs,
    /// Overrides metrics config.
    #[clap(flatten)]
    metrics_args: MetricsArgs,
}

fn main() -> Result<()> {
    let Args {
        config,
        da_args,
        log_args,
        http_args,
        network_args,
        consensus_args,
        overlay_args,
        metrics_args,
    } = Args::parse();

    let config = serde_yaml::from_reader::<_, Config>(std::fs::File::open(config)?)?
        .update_da(da_args)?
        .update_log(log_args)?
        .update_http(http_args)?
        .update_consensus(consensus_args)?
        .update_overlay(overlay_args)?
        .update_network(network_args)?;

    nomos_node::run(
        config,
        #[cfg(feature = "metrics")]
        metrics_args,
    )
}
