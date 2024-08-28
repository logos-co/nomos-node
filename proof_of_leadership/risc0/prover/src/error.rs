use thiserror::Error;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("risc0 failed to serde")]
    Risc0Serde(#[from] risc0_zkvm::serde::Error),
}
