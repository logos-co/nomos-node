#[cfg(feature = "raptor")]
pub mod raptorq;

// std
use std::error::Error;
// crates
use async_trait;
use bytes::Bytes;
use futures::Stream;
use thiserror::Error;
// internal

#[derive(Error, Debug)]
#[error(transparent)]
pub struct FountainError(#[from] Box<dyn Error + Send + Sync>);

impl From<&str> for FountainError {
    fn from(value: &str) -> Self {
        FountainError(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            value,
        )))
    }
}

#[async_trait::async_trait]
pub trait FountainCode {
    type Settings;
    fn encode(
        block: &[u8],
        settings: &Self::Settings,
    ) -> Box<dyn Stream<Item = Bytes> + Send + Sync + Unpin>;
    async fn decode(
        stream: impl Stream<Item = Bytes> + Send + Sync + Unpin,
        settings: &Self::Settings,
    ) -> Result<Bytes, FountainError>;
}
