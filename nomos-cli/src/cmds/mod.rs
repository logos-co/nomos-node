pub mod executor;

// std
// crates
use clap::Subcommand;
// internal

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Send data to the executor for encoding and dispersal.
    Disseminate(executor::Disseminate),
    Retrieve(executor::Retrieve),
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Command::Disseminate(cmd) => cmd.run(),
            Command::Retrieve(cmd) => cmd.run(),
        }?;
        Ok(())
    }
}
