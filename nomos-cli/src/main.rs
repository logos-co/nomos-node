use clap::Parser;
use nomos_cli::Cli;

fn main() {
    let cli = Cli::parse();
    cli.run().unwrap();
}
