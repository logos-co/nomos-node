pub mod executor;
pub mod validator;

// std
// crates
use clap::Subcommand;
// internal

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
            Command::Disseminate(cmd) => cmd.run(),
            Command::Retrieve(cmd) => cmd.run(),
            Command::Reconstruct(cmd) => cmd.run(),
        }?;
        Ok(())
    }
}
