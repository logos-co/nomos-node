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

/// FountainCode trait main error type
/// Wrapper around generic whichever error type the fountain code implementation uses
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

/// [FountainCode](https://en.wikipedia.org/wiki/Fountain_code)
/// Chop a block of data into chunks and reassembling trait
#[async_trait::async_trait]
pub trait FountainCode {
    type Settings: Clone + Send + Sync + 'static;
    fn new(settings: Self::Settings) -> Self;
    /// Encode a block of data into a stream of chunks
    fn encode(&self, block: &[u8]) -> Box<dyn Stream<Item = Bytes> + Send + Sync + Unpin>;
    /// Decode a stream of chunks into a block of data
    async fn decode(
        &self,
        stream: impl Stream<Item = Bytes> + Send + Sync + Unpin,
    ) -> Result<Bytes, FountainError>;
}
