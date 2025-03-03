pub mod executor;
pub mod validator;

use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Send data to the executor for encoding and dispersal.
    Disseminate(executor::Disseminate),
    Retrieve(validator::Retrieve),
    Reconstruct(validator::Reconstruct),
}

impl Command {
    pub fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Self::Disseminate(cmd) => cmd.run(),
            Self::Retrieve(cmd) => cmd.run(),
            Self::Reconstruct(cmd) => cmd.run(),
        }?;
        Ok(())
    }
}
