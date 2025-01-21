use bytes::Bytes;
use ed25519_dalek::ed25519::signature::SignerMut;
use overwatch_rs::DynError;
use rand::rngs::OsRng;
use zeroize::Zeroizing;

use crate::secure_key::SecuredKey;

/// [`ed25519_dalek::SigningKey`] implements [`zeroize::Zeroize`] internally.
pub struct Ed25519Key(ed25519_dalek::SigningKey);

impl SecuredKey for Ed25519Key {
    fn sign(&mut self, data: Bytes) -> Result<Bytes, DynError> {
        Ok(Bytes::copy_from_slice(&self.0.sign(&data).to_bytes()))
    }

    fn as_pk(&self) -> Bytes {
        Bytes::copy_from_slice(self.0.verifying_key().as_bytes())
    }
}

impl Ed25519Key {
    pub fn generate() -> Self {
        Self(ed25519_dalek::SigningKey::generate(&mut OsRng))
    }

    pub fn as_bytes(&self) -> &ed25519_dalek::SecretKey {
        self.0.as_bytes()
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut secret_key = Zeroizing::new([0u8; 32]);
        secret_key.copy_from_slice(bytes);
        Self(ed25519_dalek::SigningKey::from_bytes(&secret_key))
    }
}
