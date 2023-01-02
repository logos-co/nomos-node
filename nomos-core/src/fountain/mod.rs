#[cfg(feature = "raptor")]
pub mod raptorq;

use bytes::Bytes;
// std
// crates
use futures::Stream;
// internal

#[async_trait::async_trait]
pub trait FountainCode {
    type Settings;
    fn encode(&self, block: &[u8], settings: &Self::Settings) -> Box<dyn Iterator<Item = Bytes>>;
    async fn decode(
        stream: impl Stream<Item = Bytes> + Send + Sync + Unpin,
        settings: &Self::Settings,
    ) -> Result<Bytes, String>;
}
