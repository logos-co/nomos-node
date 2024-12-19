//STD
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
