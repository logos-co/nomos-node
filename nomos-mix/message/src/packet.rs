use serde::{Deserialize, Serialize};

use crate::{
    keyset::KeySet,
    payload::{decrypt_payload, encrypt_payload},
    Error,
};

/// A packet that contains a header and a payload.
/// The header and payload are encrypted for the selected recipients.
/// This packet can be serialized and sent over the network.
#[derive(Debug, Serialize, Deserialize)]
pub struct Packet {
    header: Header,
    // This crate doesn't limit the payload size.
    // Users must choose the appropriate constant size and implement padding.
    payload: Vec<u8>,
}

/// The packet header
#[derive(Debug, Serialize, Deserialize)]
struct Header {
    /// The ephemeral public key for a recipient to derive the shared secret
    /// which can be used to decrypt the header and payload.
    ephemeral_public_key: x25519_dalek::PublicKey,
    // TODO: Length-preserved layered encryption on RoutingInfo
    routing_info: RoutingInfo,
}

#[derive(Debug, Serialize, Deserialize)]
struct RoutingInfo {
    // TODO: Change this to `is_final_layer: bool`
    // by implementing the length-preserved layered encryption.
    // It's not good to expose the info that how many layers remain to the intermediate recipients.
    remaining_layers: u8,
    // TODO:: Add the following fields
    // header_integrity_hamc
    // additional data (e.g. incentivization)
}

impl Packet {
    pub fn build(
        recipient_pubkeys: &[x25519_dalek::PublicKey],
        payload: &[u8],
    ) -> Result<Self, Error> {
        let ephemeral_privkey = x25519_dalek::StaticSecret::random();
        let keysets = KeySet::derive_all(recipient_pubkeys, &ephemeral_privkey);

        // Encrypt the payload with the reserve order of keysets,
        // so that the 1st recipient can decrypt the outmost layer of encryption.
        let mut payload = payload.to_owned();
        keysets
            .iter()
            .rev()
            .try_for_each(|keyset| encrypt_payload(&mut payload, &keyset.payload_lioness_key))?;

        Ok(Packet {
            header: Header {
                ephemeral_public_key: x25519_dalek::PublicKey::from(&ephemeral_privkey),
                routing_info: RoutingInfo {
                    remaining_layers: u8::try_from(recipient_pubkeys.len())
                        .map_err(|_| Error::TooManyRecipients)?,
                },
            },
            payload,
        })
    }

    pub fn unpack(
        &self,
        private_key: &x25519_dalek::StaticSecret,
    ) -> Result<UnpackedPacket, Error> {
        let shared_secret = private_key.diffie_hellman(&self.header.ephemeral_public_key);
        let keyset = KeySet::derive(shared_secret.as_bytes());

        let mut payload = self.payload.clone();
        decrypt_payload(&mut payload, &keyset.payload_lioness_key)?;

        // If this is the last layer of encryption, return the decrypted payload.
        if self.header.routing_info.remaining_layers == 1 {
            return Ok(UnpackedPacket::FullyUnpacked(payload));
        }

        // Derive the new ephemeral public key for the next recipient
        let next_ephemeral_pubkey =
            keyset.derive_next_ephemeral_public_key(&self.header.ephemeral_public_key);
        Ok(UnpackedPacket::ToForward(Packet {
            header: Header {
                ephemeral_public_key: next_ephemeral_pubkey,
                routing_info: RoutingInfo {
                    remaining_layers: self.header.routing_info.remaining_layers - 1,
                },
            },
            payload,
        }))
    }
}

pub enum UnpackedPacket {
    ToForward(Packet),
    FullyUnpacked(Vec<u8>),
}

#[cfg(test)]
mod tests {
    use nomos_core::wire;

    use super::*;

    #[test]
    fn unpack() {
        // Prepare keys of two recipients
        let recipient_privkeys = (0..2)
            .map(|_| x25519_dalek::StaticSecret::random())
            .collect::<Vec<_>>();
        let recipient_pubkeys = recipient_privkeys
            .iter()
            .map(x25519_dalek::PublicKey::from)
            .collect::<Vec<_>>();

        // Build a packet
        let payload = [10u8; 512];
        let packet = Packet::build(&recipient_pubkeys, &payload).unwrap();

        // The 1st recipient unpacks the packet
        let packet = match packet.unpack(&recipient_privkeys[0]).unwrap() {
            UnpackedPacket::ToForward(packet) => packet,
            UnpackedPacket::FullyUnpacked(_) => {
                panic!("The unpacked packet should be the ToFoward type");
            }
        };
        // The 2nd recipient unpacks the packet
        match packet.unpack(&recipient_privkeys[1]).unwrap() {
            UnpackedPacket::ToForward(_) => {
                panic!("The unpacked packet should be the FullyUnpacked type");
            }
            UnpackedPacket::FullyUnpacked(unpacked_payload) => {
                // Check if the payload has been decrypted correctly
                assert_eq!(unpacked_payload, payload);
            }
        }
    }

    #[test]
    fn unpack_with_wrong_keys() {
        // Build a packet with two public keys
        let payload = [10u8; 512];
        let packet = Packet::build(
            &(0..2)
                .map(|_| x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::random()))
                .collect::<Vec<_>>(),
            &payload,
        )
        .unwrap();

        // The 1st recipient unpacks the packet with an wrong key
        let packet = match packet
            .unpack(&x25519_dalek::StaticSecret::random())
            .unwrap()
        {
            UnpackedPacket::ToForward(packet) => packet,
            UnpackedPacket::FullyUnpacked(_) => {
                panic!("The unpacked packet should be the ToFoward type");
            }
        };
        // The 2nd recipient unpacks the packet with an wrong key
        match packet
            .unpack(&x25519_dalek::StaticSecret::random())
            .unwrap()
        {
            UnpackedPacket::ToForward(_) => {
                panic!("The unpacked packet should be the FullyUnpacked type");
            }
            UnpackedPacket::FullyUnpacked(unpacked_payload) => {
                // Check if the payload has been decrypted wrongly
                assert_ne!(unpacked_payload, payload);
            }
        }
    }

    #[test]
    fn consistent_size_serialization() {
        // Prepare keys of two recipients
        let recipient_privkeys = (0..2)
            .map(|_| x25519_dalek::StaticSecret::random())
            .collect::<Vec<_>>();
        let recipient_pubkeys = recipient_privkeys
            .iter()
            .map(x25519_dalek::PublicKey::from)
            .collect::<Vec<_>>();

        // Build a packet
        let payload = [10u8; 512];
        let packet = Packet::build(&recipient_pubkeys, &payload).unwrap();

        // Calculate the expected packet size
        let pubkey_size = 32;
        let routing_info_size = 1;
        let payload_length_enconding_size = 8;
        let payload_size = 512;
        let packet_size =
            pubkey_size + routing_info_size + payload_length_enconding_size + payload_size;

        // The serialized packet size must be the same as the expected size.
        assert_eq!(wire::serialize(&packet).unwrap().len(), packet_size);

        // The unpacked packet size must be the same as the original packet size.
        match packet.unpack(&recipient_privkeys[0]).unwrap() {
            UnpackedPacket::ToForward(packet) => {
                assert_eq!(wire::serialize(&packet).unwrap().len(), packet_size);
            }
            UnpackedPacket::FullyUnpacked(_) => {
                panic!("The unpacked packet should be the ToFoward type");
            }
        }
    }
}
