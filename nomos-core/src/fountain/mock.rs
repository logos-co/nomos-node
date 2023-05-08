// std
// crates
use async_trait::async_trait;
use bytes::Bytes;
use futures::{Stream, StreamExt};
// internal
use crate::fountain::{FountainCode, FountainError};

/// Fountain code that does no protocol at all.
/// Just bypasses the raw bytes into a single chunk and reconstruct from it.
#[derive(Debug, Clone)]
pub struct MockFountain;

#[async_trait]
impl FountainCode for MockFountain {
    type Settings = ();

    fn new(_: Self::Settings) -> Self {
        Self
    }

    fn encode(&self, block: &[u8]) -> Box<dyn Stream<Item = Bytes> + Send + Sync + Unpin> {
        let data = block.to_vec();
        Box::new(futures::stream::once(Box::pin(
            async move { Bytes::from(data) },
        )))
    }

    async fn decode(
        &self,
        mut stream: impl Stream<Item = Bytes> + Send + Sync + Unpin,
    ) -> Result<Bytes, FountainError> {
        if let Some(chunk) = stream.next().await {
            Ok(chunk)
        } else {
            Err("Stream ended before decoding was complete".into())
        }
    }
}
