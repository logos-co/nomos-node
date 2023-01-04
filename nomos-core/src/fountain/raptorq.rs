// std
// crates
use bytes::Bytes;
use futures::{Stream, StreamExt};
use raptorq::{Decoder, Encoder, EncodingPacket, ObjectTransmissionInformation};
// internal
use crate::fountain::{FountainCode, FountainError};

/// [RaptorQ](https://en.wikipedia.org/wiki/Raptor_code#RaptorQ_code) implementation of [`FountainCode`] trait
pub struct RaptorQFountain {
    settings: RaptorQSettings,
}

/// Settings for [`RaptorQFountain`] code
#[derive(Clone, Debug)]
pub struct RaptorQSettings {
    pub transmission_information: ObjectTransmissionInformation,
    pub repair_packets_per_block: u32,
}

/// RaptorQ implementation of [`FountainCode`] trait
/// Wrapper around the [`raptorq`](https://crates.io/crates/raptorq) crate
#[async_trait::async_trait]
impl FountainCode for RaptorQFountain {
    type Settings = RaptorQSettings;
    fn new(settings: Self::Settings) -> Self {
        Self { settings }
    }
    fn encode(&self, block: &[u8]) -> Box<dyn Stream<Item = Bytes> + Send + Sync + Unpin> {
        let encoder = Encoder::new(block, self.settings.transmission_information);
        Box::new(futures::stream::iter(
            encoder
                .get_encoded_packets(self.settings.repair_packets_per_block)
                .into_iter()
                .map(|packet| packet.serialize().into()),
        ))
    }

    async fn decode(
        &self,
        mut stream: impl Stream<Item = Bytes> + Send + Sync + Unpin,
    ) -> Result<Bytes, FountainError> {
        let mut decoder = Decoder::new(self.settings.transmission_information);
        while let Some(chunk) = stream.next().await {
            let packet = EncodingPacket::deserialize(&chunk);
            if let Some(result) = decoder.decode(packet) {
                return Ok(Bytes::from(result));
            }
        }
        Err("Stream ended before decoding was complete".into())
    }
}

#[cfg(test)]
mod test {
    use crate::fountain::raptorq::RaptorQFountain;
    use crate::fountain::{FountainCode, FountainError};
    use bytes::Bytes;
    use futures::StreamExt;
    use rand::RngCore;

    #[tokio::test]
    async fn random_encode_decode() -> Result<(), FountainError> {
        const TRANSFER_LENGTH: usize = 1024;
        // build settings
        let settings = super::RaptorQSettings {
            transmission_information: raptorq::ObjectTransmissionInformation::with_defaults(
                TRANSFER_LENGTH as u64,
                1000,
            ),
            repair_packets_per_block: 10,
        };

        let raptor = RaptorQFountain::new(settings);

        // create random payload
        let mut payload = [0u8; TRANSFER_LENGTH];
        rand::thread_rng().fill_bytes(&mut payload);
        let payload = Bytes::from(payload.to_vec());

        // encode payload
        let encoded = raptor.encode(&payload);

        // reconstruct
        let decoded = raptor.decode(encoded).await?;

        assert_eq!(decoded, payload);
        Ok(())
    }

    #[tokio::test]
    async fn random_encode_decode_fails() {
        const TRANSFER_LENGTH: usize = 1024;
        // build settings
        let settings = super::RaptorQSettings {
            transmission_information: raptorq::ObjectTransmissionInformation::with_defaults(
                TRANSFER_LENGTH as u64,
                1000,
            ),
            repair_packets_per_block: 10,
        };

        // create random payload
        let mut payload = [0u8; TRANSFER_LENGTH];
        rand::thread_rng().fill_bytes(&mut payload);
        let payload = Bytes::from(payload.to_vec());

        let raptor = RaptorQFountain::new(settings);

        // encode payload
        let encoded = raptor.encode(&payload);

        // reconstruct skipping packets, must fail
        let decoded = raptor.decode(encoded.skip(400)).await;
        assert!(decoded.is_err());
    }
}
