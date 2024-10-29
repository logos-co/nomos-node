use rand_chacha::{
    rand_core::{RngCore, SeedableRng},
    ChaCha12Rng,
};
use sphinx_packet::{
    constants::HEADER_INTEGRITY_MAC_SIZE,
    crypto::STREAM_CIPHER_INIT_VECTOR,
    header::{
        keys::{HeaderIntegrityMacKey, RoutingKeys, StreamCipherKey},
        mac::HeaderIntegrityMac,
        routing::{RoutingFlag, FINAL_HOP, FORWARD_HOP},
    },
};

use crate::Error;

#[derive(Debug)]
pub(crate) struct EncryptedRoutingInfo {
    pub integrity_mac: HeaderIntegrityMac,
    pub encrypted_routing_info: Vec<u8>,
}

impl EncryptedRoutingInfo {
    pub fn new(routing_keys: &[RoutingKeys], max_layers: usize) -> Self {
        // Build the final routing info first, which will be placed in the inner-most layer
        let encrypted_final_routing_info = Self::build_final(
            routing_keys
                .last()
                .expect("routing_keys shouldn't be empty"),
            &new_fillers(&routing_keys[..routing_keys.len() - 1], max_layers),
            max_layers,
        );

        // Build layers on top of the final layer.
        // Use routing keys except the last one that was already used to build the final layer.
        // Routing keys must be used in reverse order to build layers from inside out.
        routing_keys.iter().take(routing_keys.len() - 1).rev().fold(
            encrypted_final_routing_info,
            |encrypted_routing_info, routing_key| {
                Self::build(routing_key, encrypted_routing_info, max_layers)
            },
        )
    }

    /// Build [`EncryptedRoutingInfo`] that can be decrypted by the recipient
    /// who can derive the `routing_key`.
    /// The `next_encrypted_routing_info` is what should be sent to the next recipient.
    fn build(
        routing_key: &RoutingKeys,
        next_encrypted_routing_info: EncryptedRoutingInfo,
        max_layers: usize,
    ) -> Self {
        // Build a routing info that contains a flag
        // and a next encrypted routing info to be send to the next recipient.
        let routing_info = RoutingInfo::new(FORWARD_HOP, next_encrypted_routing_info).into_bytes();

        // Encrypt the routing info with the key of the current recipient
        // and calculate MAC.
        let mut encrypted = routing_info;
        apply_streamcipher(
            &mut encrypted,
            &routing_key.stream_cipher_key,
            max_layers,
            false,
        );
        let mac = compute_mac(&routing_key.header_integrity_hmac_key, &encrypted);

        Self {
            integrity_mac: mac,
            encrypted_routing_info: encrypted,
        }
    }

    /// Build [`EncryptedRoutingInfo`] that can be decrypted by the last recipient
    /// who can derive the `routing_key`.
    /// The `filler` must be used for the length-preserving decryption.
    fn build_final(routing_key: &RoutingKeys, filler: &[u8], max_layers: usize) -> Self {
        // Build a routing info with a FINAL flag and a dummy random bytes
        // for the next routing info because this is for the last recipient.
        let routing_info = RoutingInfo::new(
            FINAL_HOP,
            EncryptedRoutingInfo {
                integrity_mac: HeaderIntegrityMac::from_bytes(
                    random_bytes(HEADER_INTEGRITY_MAC_SIZE).try_into().unwrap(),
                ),
                encrypted_routing_info: random_bytes(RoutingInfo::size(max_layers)),
            },
        )
        .into_bytes();

        // Encrypt the routing info with the key of the last recipient.
        let mut encrypted = routing_info;
        apply_streamcipher(
            &mut encrypted,
            &routing_key.stream_cipher_key,
            max_layers,
            false,
        );

        // Replace the last bytes with the filler.
        // This is necessary for the length-preserving decryption.
        let i = encrypted.len() - filler.len();
        encrypted[i..].copy_from_slice(filler);

        // Compute a MAC on the filler-combined encrypted routing info.
        let mac = compute_mac(&routing_key.header_integrity_hmac_key, &encrypted);

        Self {
            integrity_mac: mac,
            encrypted_routing_info: encrypted,
        }
    }

    /// Verify the integrity of the encrypted routing info.
    /// This returns [`Error::IntegrityMacVerificationFailed`] if a wrong key is used.
    pub fn verify_integrity(&self, key: HeaderIntegrityMacKey) -> Result<(), Error> {
        if !self.integrity_mac.verify(key, &self.encrypted_routing_info) {
            return Err(Error::IntegrityMacVerificationFailed);
        }
        Ok(())
    }

    /// Unpack one layer of encryption.
    pub fn unpack(&self, key: &StreamCipherKey, max_layers: usize) -> Result<Option<Self>, Error> {
        // Extend the encrypted routing info by one unit size with zeros.
        // These zeros will be restored to one step of filler by the decryption right below.
        let padded: Vec<u8> = self
            .encrypted_routing_info
            .iter()
            .cloned()
            .chain(std::iter::repeat(0u8).take(RoutingInfo::unit_size()))
            .collect();

        // Decrypt the padded routing info.
        let mut decrypted = padded;
        apply_streamcipher(&mut decrypted, key, max_layers, false);

        // Process the decrypted routing info according to the flag.
        match decrypted[0] {
            FORWARD_HOP => Ok(Some(RoutingInfo::from_extended_bytes(
                &decrypted, max_layers,
            )?)),
            FINAL_HOP => Ok(None),
            invalid_flag => Err(Error::InvalidRoutingFlag(invalid_flag)),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.integrity_mac
            .as_bytes()
            .iter()
            .cloned()
            .chain(self.encrypted_routing_info.iter().cloned())
            .collect::<Vec<_>>()
    }

    pub fn from_bytes(data: &[u8], max_layers: usize) -> Result<Self, Error> {
        if data.len() != Self::size(max_layers) {
            return Err(Error::InvalidRoutingInfoLength(data.len()));
        }

        let integrity_mac = HeaderIntegrityMac::from_bytes(
            data[..HEADER_INTEGRITY_MAC_SIZE]
                .try_into()
                .expect("The length of data must be <= {HEADER_INTEGRITY_MAC_SIZE}"),
        );
        let encrypted_routing_info = data[HEADER_INTEGRITY_MAC_SIZE..].to_vec();
        Ok(Self {
            integrity_mac,
            encrypted_routing_info,
        })
    }

    pub fn size(max_layers: usize) -> usize {
        HEADER_INTEGRITY_MAC_SIZE + RoutingInfo::size(max_layers)
    }
}

/// A plain routing info that contains metadata (only flag for now)
/// and the next encrypted routing info to be sent to the next recipient.
struct RoutingInfo {
    flag: RoutingFlag,
    next_integrity_mac: HeaderIntegrityMac,
    next_truncated_encrypted_routing_info: Vec<u8>,
}

impl RoutingInfo {
    /// The unit size that represents the size of one layer of routing info,
    /// which excludes the next encrypted routing info.
    /// Please note that the total routing info consists of multiple layers.
    #[inline]
    fn unit_size() -> usize {
        std::mem::size_of::<RoutingFlag>() + HEADER_INTEGRITY_MAC_SIZE
    }

    /// The size of an entire routing info
    /// which contains the encrypted routing info of all layers.
    fn size(max_layers: usize) -> usize {
        Self::unit_size() * max_layers
    }

    fn new(flag: RoutingFlag, next_encrypted_routing_info: EncryptedRoutingInfo) -> Self {
        let EncryptedRoutingInfo {
            integrity_mac,
            encrypted_routing_info,
        } = next_encrypted_routing_info;

        Self {
            flag,
            next_integrity_mac: integrity_mac,
            // Truncate the last bytes by the unit size, which is actually an encrypted filler.
            next_truncated_encrypted_routing_info: encrypted_routing_info
                [..encrypted_routing_info.len() - Self::unit_size()]
                .into(),
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        std::iter::once(self.flag)
            .chain(self.next_integrity_mac.into_inner())
            .chain(self.next_truncated_encrypted_routing_info.iter().cloned())
            .collect()
    }

    /// Parse [`EncryptedRoutingInfo`] from the extended bytes.
    /// The length of the extended bytes must be one unit size longer than the RoutingInfo size
    /// because it must contain one filler at its end.
    fn from_extended_bytes(data: &[u8], max_layers: usize) -> Result<EncryptedRoutingInfo, Error> {
        if data.len() != Self::size(max_layers) + Self::unit_size() {
            return Err(Error::InvalidRoutingInfoLength(data.len()));
        }

        let _flag = data[0];

        let mut i = 1;

        let next_integrity_mac = HeaderIntegrityMac::from_bytes(
            data[i..i + HEADER_INTEGRITY_MAC_SIZE]
                .try_into()
                .expect("The length of header_integrity_mac must be {HEADER_INTEGRITY_MAC_SIZE}"),
        );
        i += HEADER_INTEGRITY_MAC_SIZE;

        let next_encrypted_routing_info = data[i..].to_vec();

        Ok(EncryptedRoutingInfo {
            integrity_mac: next_integrity_mac,
            encrypted_routing_info: next_encrypted_routing_info,
        })
    }
}

fn apply_streamcipher(
    data: &mut [u8],
    key: &StreamCipherKey,
    max_layers: usize,
    apply_from_back: bool,
) {
    let pseudorandom_bytes = sphinx_packet::crypto::generate_pseudorandom_bytes(
        key,
        &STREAM_CIPHER_INIT_VECTOR,
        RoutingInfo::size(max_layers) + RoutingInfo::unit_size(),
    );
    let pseudorandom_bytes = if apply_from_back {
        &pseudorandom_bytes[pseudorandom_bytes.len() - data.len()..]
    } else {
        &pseudorandom_bytes[..data.len()]
    };
    xor(data, pseudorandom_bytes)
}

fn xor(a: &mut [u8], b: &[u8]) {
    assert_eq!(a.len(), b.len());
    a.iter_mut().zip(b.iter()).for_each(|(x1, &x2)| *x1 ^= x2);
}

fn compute_mac(key: &HeaderIntegrityMacKey, data: &[u8]) -> HeaderIntegrityMac {
    let mac = sphinx_packet::crypto::compute_keyed_hmac::<sha2::Sha256>(key, data).into_bytes();
    assert!(mac.len() >= HEADER_INTEGRITY_MAC_SIZE);
    HeaderIntegrityMac::from_bytes(
        mac.into_iter()
            .take(HEADER_INTEGRITY_MAC_SIZE)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap(),
    )
}

/// Build as many fillers as the number of routing keys provided.
/// Fillers are encrypted in accumulated manner by routing keys.
fn new_fillers(routing_keys: &[RoutingKeys], max_layers: usize) -> Vec<u8> {
    routing_keys
        .iter()
        .fold(Vec::new(), |mut fillers, routing_key| {
            fillers.extend(vec![0u8; RoutingInfo::unit_size()]);
            apply_streamcipher(
                &mut fillers,
                &routing_key.stream_cipher_key,
                max_layers,
                true,
            );
            fillers
        })
}

fn random_bytes(size: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; size];
    let mut rng = ChaCha12Rng::from_entropy();
    rng.fill_bytes(&mut bytes);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::Packet;

    #[test]
    fn encapsulate() {
        // Prepare keys of two recipients
        let recipient_privkeys = (0..2)
            .map(|_| x25519_dalek::StaticSecret::random())
            .collect::<Vec<_>>();
        let recipient_pubkeys = recipient_privkeys
            .iter()
            .map(x25519_dalek::PublicKey::from)
            .collect::<Vec<_>>();
        let ephemeral_privkey = x25519_dalek::StaticSecret::random();
        let key_material = Packet::derive_key_material(&recipient_pubkeys, &ephemeral_privkey);

        let max_layers = 5;

        let encrypted_routing_info =
            EncryptedRoutingInfo::new(&key_material.routing_keys, max_layers);

        let encrypted_routing_info = match encrypted_routing_info
            .unpack(&key_material.routing_keys[0].stream_cipher_key, max_layers)
            .unwrap()
        {
            Some(enc_routing_info) => enc_routing_info,
            None => panic!("Next routing info is expected"),
        };

        assert!(encrypted_routing_info
            .unpack(&key_material.routing_keys[1].stream_cipher_key, max_layers)
            .unwrap()
            .is_none());
    }
}
