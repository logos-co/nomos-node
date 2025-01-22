use std::collections::HashMap;

use bytes::Bytes;
use either::Either;
use overwatch_rs::DynError;
use serde::{Deserialize, Serialize};
use zeroize::ZeroizeOnDrop;

use crate::{keys::ed25519::Ed25519Key, secure_key::SecuredKey, KMSOperator};

use super::KMSBackend;

pub struct PreloadKMSBackend {
    keys: HashMap<String, Key>,
}

/// This settings contain all [`Key`]s to be loaded into the [`PreloadKMSBackend`].
/// This implements [`serde::Serialize`] for users to populate the settings from bytes.
/// The [`Key`] also implements [`zeroize::ZeroizeOnDrop`] for security.
#[derive(Serialize, Deserialize)]
pub struct PreloadKMSBackendSettings {
    keys: HashMap<String, Key>,
}

#[async_trait::async_trait]
impl KMSBackend for PreloadKMSBackend {
    type SupportedKeys = SupportedKeys;
    type KeyId = String;
    type Settings = PreloadKMSBackendSettings;

    fn new(settings: Self::Settings) -> Self {
        Self {
            keys: settings.keys,
        }
    }

    /// This function just checks if the key_id was preloaded successfully.
    /// This backend doesn't support generating a new key based on the [`SupportedKeys`] provided,
    /// because this backend preloads all keys from the settings.
    /// In other words, the users can execute operations with the key_id
    /// even without the need to register it, if the key was specified in the settings.
    fn register(
        &mut self,
        key: Either<Self::SupportedKeys, Self::KeyId>,
    ) -> Result<Self::KeyId, DynError> {
        match key {
            Either::Left(_) => Err(Error::NotSupported.into()),
            Either::Right(key_id) => {
                if self.keys.contains_key(&key_id) {
                    Ok(key_id)
                } else {
                    Err(Error::KeyNotRegistered(key_id).into())
                }
            }
        }
    }

    fn public_key(&self, key_id: Self::KeyId) -> Result<Bytes, DynError> {
        Ok(self
            .keys
            .get(&key_id)
            .ok_or(Error::KeyNotRegistered(key_id))?
            .as_pk())
    }

    fn sign(&mut self, key_id: Self::KeyId, data: Bytes) -> Result<Bytes, DynError> {
        self.keys
            .get_mut(&key_id)
            .ok_or(Error::KeyNotRegistered(key_id))?
            .sign(data)
    }

    async fn execute(&mut self, key_id: Self::KeyId, mut op: KMSOperator) -> Result<(), DynError> {
        op(self
            .keys
            .get_mut(&key_id)
            .ok_or(Error::KeyNotRegistered(key_id))?)
        .await
    }
}

// This enum won't be used outside of this module
// because [`PreloadKMSBackend`] doesn't support generating new keys.
#[allow(dead_code)]
pub enum SupportedKeys {
    Ed25519,
}

#[derive(Serialize, Deserialize, ZeroizeOnDrop)]
enum Key {
    Ed25519(Ed25519Key),
}

impl SecuredKey for Key {
    fn sign(&mut self, data: Bytes) -> Result<Bytes, DynError> {
        match self {
            Self::Ed25519(key) => key.sign(data),
        }
    }

    fn as_pk(&self) -> Bytes {
        match self {
            Self::Ed25519(key) => key.as_pk(),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Not supported")]
    NotSupported,
    #[error("Key({0}) was not registered")]
    KeyNotRegistered(String),
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rand::rngs::OsRng;

    use super::*;

    #[tokio::test]
    async fn preload_backend() {
        // Initialize a backend with a pre-generated key in the setting
        let key_id = "blend/1".to_string();
        let key = ed25519_dalek::SigningKey::generate(&mut OsRng);
        let mut backend = PreloadKMSBackend::new(PreloadKMSBackendSettings {
            keys: HashMap::from_iter(vec![(
                key_id.clone(),
                Key::Ed25519(Ed25519Key(key.clone())),
            )]),
        });

        // Check if the key was preloaded successfully
        assert_eq!(
            backend.register(Either::Right(key_id.clone())).unwrap(),
            key_id
        );

        // Check if the result of key operations of the backend are the same as
        // the direct operation on the key itself.
        let mut key = Ed25519Key(key.clone());
        assert_eq!(backend.public_key(key_id.clone()).unwrap(), key.as_pk());
        let data = Bytes::from("data");
        assert_eq!(
            backend.sign(key_id.clone(), data.clone()).unwrap(),
            key.sign(data).unwrap()
        );

        // Check if the execute function works as expected
        backend
            .execute(
                key_id.clone(),
                Box::new(move |_: &mut dyn SecuredKey| Box::pin(async move { Ok(()) })),
            )
            .await
            .unwrap();
    }

    #[test]
    fn key_generation_not_supported() {
        let mut backend = PreloadKMSBackend::new(PreloadKMSBackendSettings {
            keys: HashMap::new(),
        });

        // This backend shouldn't support generating new keys.
        assert!(backend
            .register(Either::Left(SupportedKeys::Ed25519))
            .is_err());
    }

    #[tokio::test]
    async fn key_not_registered() {
        let mut backend = PreloadKMSBackend::new(PreloadKMSBackendSettings {
            keys: HashMap::from_iter(vec![(
                "blend/1".to_string(),
                Key::Ed25519(Ed25519Key(ed25519_dalek::SigningKey::generate(&mut OsRng))),
            )]),
        });

        let unknown_key = "blend/not_registered".to_string();
        assert!(backend.public_key(unknown_key.clone()).is_err());
        assert!(backend
            .sign(unknown_key.clone(), Bytes::from("data"))
            .is_err());
        assert!(backend
            .execute(
                unknown_key,
                Box::new(move |_: &mut dyn SecuredKey| Box::pin(async move { Ok(()) })),
            )
            .await
            .is_err());
    }
}
