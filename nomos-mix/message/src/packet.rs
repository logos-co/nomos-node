use crate::Error;
use serde::{Deserialize, Serialize};
use sphinx_packet::constants::NODE_ADDRESS_LENGTH;

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
        payload_size: usize,
    ) -> Result<Self, Error> {
        // Derive `[sphinx_packet::header::keys::KeyMaterial]` for all recipients.
        let ephemeral_privkey = x25519_dalek::StaticSecret::random();
        let key_material = Self::derive_key_material(recipient_pubkeys, &ephemeral_privkey);

        // Encrypt the payload for all recipients.
        let payload_keys = key_material
            .routing_keys
            .iter()
            .map(|key_material| key_material.payload_key)
            .collect::<Vec<_>>();
        let payload = sphinx_packet::payload::Payload::encapsulate_message(
            payload,
            &payload_keys,
            payload_size,
        )?;

        Ok(Packet {
            header: Header {
                ephemeral_public_key: x25519_dalek::PublicKey::from(&ephemeral_privkey),
                routing_info: RoutingInfo {
                    remaining_layers: u8::try_from(recipient_pubkeys.len())
                        .map_err(|_| Error::InvalidNumberOfLayers)?,
                },
            },
            payload: payload.into_bytes(),
        })
    }

    fn derive_key_material(
        recipient_pubkeys: &[x25519_dalek::PublicKey],
        ephemeral_privkey: &x25519_dalek::StaticSecret,
    ) -> sphinx_packet::header::keys::KeyMaterial {
        // NodeAddress is needed to build [`sphinx_packet::route::Node`]s
        // required by [`sphinx_packet::header::keys::KeyMaterial::derive`],
        // but it's not actually used inside. So, we can use a dummy address.
        let dummy_node_address =
            sphinx_packet::route::NodeAddressBytes::from_bytes([0u8; NODE_ADDRESS_LENGTH]);
        let route = recipient_pubkeys
            .iter()
            .map(|pubkey| sphinx_packet::route::Node {
                address: dummy_node_address,
                pub_key: *pubkey,
            })
            .collect::<Vec<_>>();
        sphinx_packet::header::keys::KeyMaterial::derive(&route, ephemeral_privkey)
    }

    pub fn unpack(
        &self,
        private_key: &x25519_dalek::StaticSecret,
    ) -> Result<UnpackedPacket, Error> {
        // Derive the routing keys for the recipient
        let routing_keys = sphinx_packet::header::SphinxHeader::compute_routing_keys(
            &self.header.ephemeral_public_key,
            private_key,
        );

        // Decrypt one layer of encryption on the payload
        let payload = sphinx_packet::payload::Payload::from_bytes(&self.payload)?;
        let payload = payload.unwrap(&routing_keys.payload_key)?;

        // If this is the last layer of encryption, return the decrypted payload.
        if self.header.routing_info.remaining_layers == 1 {
            return Ok(UnpackedPacket::FullyUnpacked(payload.recover_plaintext()?));
        }

        // Derive the new ephemeral public key for the next recipient
        let next_ephemeral_pubkey = Self::derive_next_ephemeral_public_key(
            &self.header.ephemeral_public_key,
            &routing_keys.blinding_factor,
        );
        Ok(UnpackedPacket::ToForward(Packet {
            header: Header {
                ephemeral_public_key: next_ephemeral_pubkey,
                routing_info: RoutingInfo {
                    remaining_layers: self.header.routing_info.remaining_layers - 1,
                },
            },
            payload: payload.into_bytes(),
        }))
    }

    /// Derive the next ephemeral public key for the next recipient.
    //
    // This is a copy of `blind_the_shared_secret` from https://github.com/nymtech/sphinx/blob/344b902df340e0d5af69c5147b05f76f324b8cef/src/header/mod.rs#L234.
    // with renaming the function name and the arguments
    // because the original function is not exposed to the public.
    // This logic is tightly coupled with the [`sphinx_packet::header::keys::KeyMaterial::derive`].
    fn derive_next_ephemeral_public_key(
        cur_ephemeral_pubkey: &x25519_dalek::PublicKey,
        blinding_factor: &x25519_dalek::StaticSecret,
    ) -> x25519_dalek::PublicKey {
        let new_shared_secret = blinding_factor.diffie_hellman(cur_ephemeral_pubkey);
        x25519_dalek::PublicKey::from(new_shared_secret.to_bytes())
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
        let packet = Packet::build(&recipient_pubkeys, &payload, 1024).unwrap();

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
            1024,
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
        assert!(packet
            .unpack(&x25519_dalek::StaticSecret::random())
            .is_err());
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
        let packet = Packet::build(&recipient_pubkeys, &payload, 1024).unwrap();

        // Calculate the expected packet size
        let pubkey_size = 32;
        let routing_info_size = 1;
        let payload_length_enconding_size = 8;
        let payload_size = 1024;
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
