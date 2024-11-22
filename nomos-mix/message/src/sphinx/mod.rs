use error::Error;
use packet::{Packet, UnpackedPacket};
use serde::{Deserialize, Serialize};

use crate::MixMessage;

pub mod error;
mod layered_cipher;
pub mod packet;
mod routing;

pub struct SphinxMessage {
    settings: SphinxMessageSettings,
    drop_message: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SphinxMessageSettings {
    max_layers: usize,
    max_payload_size: usize,
}

const ASYM_KEY_SIZE: usize = 32;

impl MixMessage for SphinxMessage {
    type PublicKey = [u8; ASYM_KEY_SIZE];
    type PrivateKey = [u8; ASYM_KEY_SIZE];
    type Settings = SphinxMessageSettings;
    type Error = Error;

    fn new(settings: Self::Settings) -> Self {
        let drop_message = vec![0; Packet::size(settings.max_layers, settings.max_payload_size)];
        Self {
            settings,
            drop_message,
        }
    }

    fn build_message(
        &self,
        payload: &[u8],
        public_keys: &[Self::PublicKey],
    ) -> Result<Vec<u8>, Self::Error> {
        let packet = Packet::build(
            &public_keys
                .iter()
                .map(|k| x25519_dalek::PublicKey::from(*k))
                .collect::<Vec<_>>(),
            self.settings.max_layers,
            payload,
            self.settings.max_payload_size,
        )?;
        Ok(packet.to_bytes())
    }

    fn unwrap_message(
        &self,
        message: &[u8],
        private_key: &Self::PrivateKey,
    ) -> Result<(Vec<u8>, bool), Self::Error> {
        let packet = Packet::from_bytes(message, self.settings.max_layers)?;
        let unpacked_packet = packet.unpack(
            &x25519_dalek::StaticSecret::from(*private_key),
            self.settings.max_layers,
        )?;
        match unpacked_packet {
            UnpackedPacket::ToForward(packet) => Ok((packet.to_bytes(), false)),
            UnpackedPacket::FullyUnpacked(payload) => Ok((payload, true)),
        }
    }

    fn drop_message(&self) -> &[u8] {
        &self.drop_message
    }
}

fn parse_bytes<'a>(data: &'a [u8], sizes: &[usize]) -> Result<Vec<&'a [u8]>, String> {
    let mut i = 0;
    sizes
        .iter()
        .map(|&size| {
            if i + size > data.len() {
                return Err("The sum of sizes exceeds the length of the input slice".to_string());
            }
            let slice = &data[i..i + size];
            i += size;
            Ok(slice)
        })
        .collect()
}
