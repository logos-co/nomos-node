use std::{fmt::Debug, ops::Deref};

/// Private key for decrypting Sphinx packets.
///
/// The key type is basically X25519, but with small modifications: https://github.com/nymtech/sphinx/blob/ca107d94360cdf8bbfbdb12fe5320ed74f80e40c/src/crypto/keys.rs#L15
pub struct PrivateKey(sphinx_packet::crypto::PrivateKey);

impl PrivateKey {
    /// Creates a random [`PrivateKey`].
    pub fn new() -> Self {
        Self(sphinx_packet::crypto::PrivateKey::new())
    }
}

impl Debug for PrivateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PrivateKey").finish()
    }
}

impl Deref for PrivateKey {
    type Target = sphinx_packet::crypto::PrivateKey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Default for PrivateKey {
    fn default() -> Self {
        Self::new()
    }
}

/// Public key for encrypting Sphinx packets
#[derive(Clone, Debug)]
pub struct PublicKey(sphinx_packet::crypto::PublicKey);

impl Deref for PublicKey {
    type Target = sphinx_packet::crypto::PublicKey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<&PrivateKey> for PublicKey {
    fn from(private_key: &PrivateKey) -> Self {
        let key: &sphinx_packet::crypto::PrivateKey = private_key;
        Self(sphinx_packet::crypto::PublicKey::from(key))
    }
}

impl From<sphinx_packet::crypto::PublicKey> for PublicKey {
    fn from(public_key: sphinx_packet::crypto::PublicKey) -> Self {
        Self(public_key)
    }
}
