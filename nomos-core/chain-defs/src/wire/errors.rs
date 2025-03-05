use std::io;

use crate::wire::bincode::clone_bincode_error;

#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("Failed to serialize message: {0}")]
    Serialize(bincode::Error),
    #[error("Failed to deserialize message: {0}")]
    Deserialize(bincode::Error),
}

impl From<WireError> for io::Error {
    fn from(value: WireError) -> Self {
        Self::new(io::ErrorKind::InvalidData, value)
    }
}

impl Clone for WireError {
    fn clone(&self) -> Self {
        match self {
            Self::Serialize(error) => Self::Serialize(Box::new(clone_bincode_error(error))),
            Self::Deserialize(error) => Self::Deserialize(Box::new(clone_bincode_error(error))),
        }
    }
}
