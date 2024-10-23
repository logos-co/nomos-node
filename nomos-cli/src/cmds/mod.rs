pub mod executor;

use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Send data to the executor for encoding and dispersal.
    Disseminate(executor::Disseminate),
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Command::Disseminate(cmd) => cmd.run(),
        }?;
        Ok(())
    }
}
