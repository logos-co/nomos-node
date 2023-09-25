mod cmds;

use clap::Parser;
use cmds::Command;
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

fn main() {
    let cli = Cli::parse();
    cli.command.run().unwrap();
}
