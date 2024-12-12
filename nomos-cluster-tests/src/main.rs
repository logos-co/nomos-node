use clap::Parser;
use color_eyre::eyre::Result;
use nomos_cluster_tests::{
    config::{Config, LogArgs},
    test_case::data_integrity_nodes_join_leave::DataIntegrityNodesJoinLeave,
    TestCase,
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path for a yaml-encoded network config file
    config: std::path::PathBuf,
    /// Overrides log config.
    #[clap(flatten)]
    log_args: LogArgs,
}

fn main() -> Result<()> {
    // Parse cluster options
    let Args { config, log_args } = Args::parse();
    let _config = serde_yaml::from_reader::<_, Config>(std::fs::File::open(config)?)?
        .update_from_args(log_args)?;

    // Run the test suite
    let tc1 = DataIntegrityNodesJoinLeave {};
    println!("Running test: {}", tc1.name());

    Ok(())
}
