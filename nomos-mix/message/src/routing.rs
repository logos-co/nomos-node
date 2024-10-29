use sphinx_packet::{
    constants::HEADER_INTEGRITY_MAC_SIZE,
    header::{
        keys::RoutingKeys,
        mac::HeaderIntegrityMac,
        routing::{RoutingFlag, FINAL_HOP, FORWARD_HOP},
    },
};

use crate::{
    concat_bytes,
    layered_cipher::{ConsistentLengthLayeredCipher, EncryptionParam, Key},
    parse_bytes, Error,
};

/// A routing information that will be contained in a packet header
/// in the encrypted format.
pub struct RoutingInformation {
    pub flag: RoutingFlag,
    // Add additional fields here
}

impl RoutingInformation {
    pub fn new(flag: RoutingFlag) -> Self {
        Self { flag }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        vec![self.flag]
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, Error> {
        if data.len() != Self::size() {
            return Err(Error::InvalidEncryptedRoutingInfoLength(data.len()));
        }
        Ok(Self { flag: data[0] })
    }

    pub fn size() -> usize {
        std::mem::size_of::<RoutingFlag>()
    }
}

/// Encrypted routing information that will be contained in a packet header.
#[derive(Debug)]
pub struct EncryptedRoutingInformation {
    /// A MAC to verify the integrity of [`Self::encrypted_routing_info`].
    mac: HeaderIntegrityMac,
    /// The actual encrypted routing information produced by [`ConsistentLengthLayeredCipher`].
    /// Its size should be the same as [`ConsistentLengthLayeredCipher::total_size`].
    encrypted_routing_info: Vec<u8>,
}

impl EncryptedRoutingInformation {
    /// Build all [`RoutingInformation`]s for the provides keys,
    /// and encrypt them using [`ConsistentLengthLayeredCipher`].
    pub fn new(routing_keys: &[RoutingKeys], max_layers: usize) -> Result<Self, Error> {
        let cipher = Self::layered_cipher(max_layers);
        let params = routing_keys
            .iter()
            .enumerate()
            .map(|(i, k)| {
                let flag = if i == routing_keys.len() - 1 {
                    FINAL_HOP
                } else {
                    FORWARD_HOP
                };
                EncryptionParam {
                    data: RoutingInformation::new(flag).to_bytes(),
                    key: Self::layered_cipher_key(k),
                }
            })
            .collect::<Vec<_>>();
        let (encrypted, mac) = cipher.encrypt(&params)?;

        Ok(Self {
            mac,
            encrypted_routing_info: encrypted,
        })
    }

    /// Unpack one layer of encryptions using the key provided.
    /// Returns the decrypted routing information
    /// and the next [`EncryptedRoutingInformation`] to be unpacked further.
    pub fn unpack(
        &self,
        routing_key: &RoutingKeys,
        max_layers: usize,
    ) -> Result<(RoutingInformation, Self), Error> {
        let cipher = Self::layered_cipher(max_layers);
        let (routing_info, next_mac, next_encrypted_routing_info) = cipher.unpack(
            &self.mac,
            &self.encrypted_routing_info,
            &Self::layered_cipher_key(routing_key),
        )?;
        Ok((
            RoutingInformation::from_bytes(&routing_info)?,
            Self {
                mac: next_mac,
                encrypted_routing_info: next_encrypted_routing_info,
            },
        ))
    }

    fn layered_cipher(max_layers: usize) -> ConsistentLengthLayeredCipher {
        ConsistentLengthLayeredCipher::new(max_layers, RoutingInformation::size())
    }

    fn layered_cipher_key(routing_key: &RoutingKeys) -> Key {
        Key {
            stream_cipher_key: routing_key.stream_cipher_key,
            integrity_mac_key: routing_key.header_integrity_hmac_key,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        concat_bytes(&[self.mac.as_bytes(), &self.encrypted_routing_info])
    }

    pub fn from_bytes(data: &[u8], max_layers: usize) -> Result<Self, Error> {
        let parsed = parse_bytes(
            data,
            &[
                HEADER_INTEGRITY_MAC_SIZE,
                Self::layered_cipher(max_layers).total_size(),
            ],
        )
        .map_err(|_| Error::InvalidEncryptedRoutingInfoLength(data.len()))?;
        Ok(Self {
            mac: HeaderIntegrityMac::from_bytes(parsed[0].try_into().unwrap()),
            encrypted_routing_info: parsed[1].to_vec(),
        })
    }

    pub fn size(max_layers: usize) -> usize {
        let cipher = ConsistentLengthLayeredCipher::new(max_layers, RoutingInformation::size());
        HEADER_INTEGRITY_MAC_SIZE + cipher.total_size()
    }
}
