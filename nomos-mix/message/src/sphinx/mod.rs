use error::Error;
use packet::{Packet, UnpackedPacket};
use serde::{Deserialize, Serialize};

use crate::MixMessage;

pub mod error;
mod layered_cipher;
pub mod packet;
mod routing;

pub struct SphinxMessage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SphinxMessageSettings {
    pub max_layers: usize,
    pub max_payload_size: usize,
}

const ASYM_KEY_SIZE: usize = 32;

impl MixMessage for SphinxMessage {
    type PublicKey = [u8; ASYM_KEY_SIZE];
    type PrivateKey = [u8; ASYM_KEY_SIZE];
    type Settings = SphinxMessageSettings;
    type Error = Error;

    // TODO: Remove DROP_MESSAGE. Currently, an arbitrary size (2048) is used,
    // but we've decided to remove drop messages from the spec.
    const DROP_MESSAGE: &'static [u8] = &[0; 2048];

    fn build_message(
        payload: &[u8],
        public_keys: &[Self::PublicKey],
        settings: &Self::Settings,
    ) -> Result<Vec<u8>, Self::Error> {
        let packet = Packet::build(
            &public_keys
                .iter()
                .map(|k| x25519_dalek::PublicKey::from(*k))
                .collect::<Vec<_>>(),
            settings.max_layers,
            payload,
            settings.max_payload_size,
        )?;
        Ok(packet.to_bytes())
    }

    fn unwrap_message(
        message: &[u8],
        private_key: &Self::PrivateKey,
        settings: &Self::Settings,
    ) -> Result<(Vec<u8>, bool), Self::Error> {
        let packet = Packet::from_bytes(message, settings.max_layers)?;
        let unpacked_packet = packet.unpack(
            &x25519_dalek::StaticSecret::from(*private_key),
            settings.max_layers,
        )?;
        match unpacked_packet {
            UnpackedPacket::ToForward(packet) => Ok((packet.to_bytes(), false)),
            UnpackedPacket::FullyUnpacked(payload) => Ok((payload, true)),
        }
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
