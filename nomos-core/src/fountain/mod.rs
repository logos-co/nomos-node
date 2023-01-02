#[cfg(feature = "raptor")]
pub mod raptorq;
// std
// crates
use async_trait;
use bytes::Bytes;
use futures::Stream;
// internal

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
    ) -> Result<Bytes, String>;
}
