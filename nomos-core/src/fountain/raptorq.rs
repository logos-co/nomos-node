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
    fn encode(&self, block: &[u8], settings: &Self::Settings) -> Box<dyn Iterator<Item = Bytes>> {
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
            decoder.add_new_packet(packet);
            if let Some(result) = decoder.get_result() {
                return Ok(Bytes::from(result));
            }
        }
        Err("Stream ended before decoding was complete".to_string())
    }
}
