use std::{fmt::Debug, ops::Deref};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

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

impl Serialize for PrivateKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        SerializablePrivateKey::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PrivateKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::from(SerializablePrivateKey::deserialize(
            deserializer,
        )?))
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

impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        SerializablePublicKey::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::from(SerializablePublicKey::deserialize(
            deserializer,
        )?))
    }
}

// Only for serializing/deserializing [`PrivateKey`] since [`sphinx_packet::crypto::PrivateKey`] is not serializable.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct SerializablePrivateKey([u8; 32]);

impl From<&PrivateKey> for SerializablePrivateKey {
    fn from(key: &PrivateKey) -> Self {
        Self(key.0.to_bytes())
    }
}

impl From<SerializablePrivateKey> for PrivateKey {
    fn from(key: SerializablePrivateKey) -> Self {
        Self(sphinx_packet::crypto::PrivateKey::from(key.0))
    }
}

// Only for serializing/deserializing [`PublicKey`] since [`sphinx_packet::crypto::PublicKey`] is not serializable.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct SerializablePublicKey([u8; 32]);

impl From<&PublicKey> for SerializablePublicKey {
    fn from(key: &PublicKey) -> Self {
        Self(*key.0.as_bytes())
    }
}

impl From<SerializablePublicKey> for PublicKey {
    fn from(key: SerializablePublicKey) -> Self {
        Self(sphinx_packet::crypto::PublicKey::from(key.0))
    }
}
