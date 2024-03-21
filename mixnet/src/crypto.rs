use sphinx_packet::crypto::{PrivateKey, PublicKey, PRIVATE_KEY_SIZE, PUBLIC_KEY_SIZE};

/// Converts a mixnode private key to a public key
pub fn public_key_from(private_key: [u8; PRIVATE_KEY_SIZE]) -> [u8; PUBLIC_KEY_SIZE] {
    *PublicKey::from(&PrivateKey::from(private_key)).as_bytes()
}
