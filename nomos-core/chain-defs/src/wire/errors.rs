//STD
use crate::wire::bincode::clone_bincode_error;
use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to serialize message: {0}")]
    Serialize(bincode::Error),
    #[error("Failed to deserialize message: {0}")]
    Deserialize(bincode::Error),
}

impl From<Error> for io::Error {
    fn from(value: Error) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, value)
    }
}

impl Clone for Error {
    fn clone(&self) -> Self {
        match self {
            Error::Serialize(error) => Error::Serialize(clone_bincode_error(error)),
            Error::Deserialize(error) => Error::Deserialize(clone_bincode_error(error)),
        }
    }
}
