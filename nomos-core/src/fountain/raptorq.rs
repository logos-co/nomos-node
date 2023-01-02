// std
// crates
use bytes::Bytes;
use futures::{Stream, StreamExt};
use raptorq::{Decoder, Encoder, EncodingPacket, ObjectTransmissionInformation};
// internal
use crate::fountain::FountainCode;

pub struct RaptorQFountain;

pub struct RaptorQSettings {
    pub transmission_information: ObjectTransmissionInformation,
    pub repair_packets_per_block: u32,
}

#[async_trait::async_trait]
impl FountainCode for RaptorQFountain {
    type Settings = RaptorQSettings;
    fn encode(
        block: &[u8],
        settings: &Self::Settings,
    ) -> Box<dyn Iterator<Item = Bytes> + Send + Sync> {
        let encoder = Encoder::new(block, settings.transmission_information);
        Box::new(
            encoder
                .get_encoded_packets(settings.repair_packets_per_block)
                .into_iter()
                .map(|packet| packet.serialize().into()),
        )
    }

    async fn decode(
        mut stream: impl Stream<Item = Bytes> + Send + Sync + Unpin,
        settings: &Self::Settings,
    ) -> Result<Bytes, String> {
        let mut decoder = Decoder::new(settings.transmission_information);
        while let Some(chunk) = stream.next().await {
            let packet = EncodingPacket::deserialize(&chunk);
            if let Some(result) = decoder.decode(packet) {
                return Ok(Bytes::from(result));
            }
        }
        Err("Stream ended before decoding was complete".to_string())
    }
}

#[cfg(test)]
mod test {
    use crate::fountain::raptorq::RaptorQFountain;
    use crate::fountain::FountainCode;
    use bytes::Bytes;
    use rand::RngCore;

    #[tokio::test]
    async fn random_encode_decode() -> Result<(), String> {
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

        // encode payload
        let encoded = RaptorQFountain::encode(&payload, &settings);

        // reconstruct
        let decoded = RaptorQFountain::decode(futures::stream::iter(encoded), &settings).await?;

        assert_eq!(decoded, payload);
        Ok(())
    }
}
