use std::io;

use crate::wire::bincode::clone_bincode_error;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to serialize message: {0}")]
    Serialize(bincode::Error),
    #[error("Failed to deserialize message: {0}")]
    Deserialize(bincode::Error),
}

impl From<Error> for io::Error {
    fn from(value: Error) -> Self {
        Self::new(io::ErrorKind::InvalidData, value)
    }
}

impl Clone for Error {
    fn clone(&self) -> Self {
        match self {
            Self::Serialize(error) => Self::Serialize(clone_bincode_error(error)),
            Self::Deserialize(error) => Self::Deserialize(clone_bincode_error(error)),
        }
    }
}
