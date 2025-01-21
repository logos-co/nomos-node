use std::{collections::HashMap, path::PathBuf};

use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
use bytes::Bytes;
use either::Either;
use overwatch_rs::DynError;
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::{keys::ed25519::Ed25519Key, secure_key::SecuredKey, KMSOperator};

use super::KMSBackend;

type UuidBytes = [u8; 16];

pub struct FileSystemKMSBackend {
    dir: PathBuf,
    encryption_key: aes_gcm::Key<Aes256Gcm>,
    keys: HashMap<UuidBytes, Box<dyn SecuredKey + Sync + Send>>,
}

#[derive(Clone)]
pub struct FileSystemKMSBackendSettings {
    pub dir: PathBuf,
    pub password: String,
    pub salt: String,
    pub rounds: u32,
}

#[async_trait::async_trait]
impl KMSBackend for FileSystemKMSBackend {
    type SupportedKeys = SupportedKeys;
    type KeyId = UuidBytes;
    type Settings = FileSystemKMSBackendSettings;

    fn new(settings: Self::Settings) -> Self {
        // Derive an encryption key from the password using PBKDF2
        let mut derived_key = [0u8; 32];
        pbkdf2_hmac::<Sha256>(
            settings.password.as_bytes(),
            settings.salt.as_bytes(),
            settings.rounds,
            &mut derived_key,
        );
        let encryption_key = *aes_gcm::Key::<Aes256Gcm>::from_slice(&derived_key);
        derived_key.zeroize();

        Self {
            dir: settings.dir,
            encryption_key,
            keys: HashMap::new(),
        }
    }

    fn register(
        &mut self,
        key: Either<Self::SupportedKeys, Self::KeyId>,
    ) -> Result<Self::KeyId, DynError> {
        let key_id = match key {
            Either::Left(key_type) => {
                // Generate a new key
                let key_id = Uuid::new_v4();
                let (key, key_bytes) = key_type.generate();

                // Encrypt the key
                let encrypted_key = Aes256Gcm::new(&self.encryption_key)
                    .encrypt(Nonce::from_slice(key_id.as_bytes()), key_bytes.as_ref())
                    .map_err(|_| Error::EncryptionFailed)?;

                // Write the encrypted key to disk
                std::fs::write(self.dir.join(key_id.to_string()), encrypted_key)?;

                // Keep the key in memory
                let key_id = key_id.into_bytes();
                self.keys.insert(key_id, key);
                key_id
            }
            Either::Right(key_id) => {
                let encrypted_key =
                    std::fs::read(self.dir.join(Uuid::from_bytes(key_id).to_string()))?;
                let key_bytes = Zeroizing::new(
                    Aes256Gcm::new(&self.encryption_key)
                        .decrypt(Nonce::from_slice(&key_id), encrypted_key.as_ref())
                        .map_err(|_| Error::DecryptionFailed)?,
                );
                self.keys
                    .insert(key_id, Self::SupportedKeys::load(&key_bytes)?);
                key_id
            }
        };
        Ok(key_id)
    }

    fn public_key(&self, key_id: Self::KeyId) -> Result<Bytes, DynError> {
        Ok(self
            .keys
            .get(&key_id)
            .ok_or(Error::KeyIdNotRegistered(key_id))?
            .as_pk())
    }

    fn sign(&mut self, key_id: Self::KeyId, data: Bytes) -> Result<Bytes, DynError> {
        self.keys
            .get_mut(&key_id)
            .ok_or(Error::KeyIdNotRegistered(key_id))?
            .sign(data)
    }

    async fn execute(&mut self, key_id: Self::KeyId, mut op: KMSOperator) -> Result<(), DynError> {
        op(self
            .keys
            .get_mut(&key_id)
            .ok_or(Error::KeyIdNotRegistered(key_id))?)
        .await
    }
}

#[repr(u8)]
#[allow(dead_code)] // TODO: Remove this macro once this is used outside
pub enum SupportedKeys {
    Ed25519 = 0,
}

impl SupportedKeys {
    fn generate(&self) -> (Box<dyn SecuredKey + Sync + Send>, Zeroizing<Vec<u8>>) {
        match self {
            SupportedKeys::Ed25519 => {
                let key = Ed25519Key::generate();
                let mut bytes = Zeroizing::new(Vec::new());
                bytes.push(SupportedKeys::Ed25519 as u8);
                bytes.extend_from_slice(key.as_bytes());
                (Box::new(key), bytes)
            }
        }
    }

    fn load(bytes: &[u8]) -> Result<Box<dyn SecuredKey + Sync + Send>, Error> {
        match bytes.first() {
            Some(val) if *val == SupportedKeys::Ed25519 as u8 => Ok(Box::new(
                Ed25519Key::from_bytes(bytes.get(1..).ok_or(Error::InvalidKeyBytes)?),
            )),
            Some(_) => Err(Error::UnknownKeyType(bytes[0])),
            None => Err(Error::InvalidKeyBytes),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Encryption failed")]
    EncryptionFailed,
    #[error("Decryption failed")]
    DecryptionFailed,
    #[error("Unknown key type: {0}")]
    UnknownKeyType(u8),
    #[error("Invalid key bytes")]
    InvalidKeyBytes,
    #[error("Key ID not registered: {0:?}")]
    KeyIdNotRegistered(UuidBytes),
}

#[cfg(test)]
mod tests {
    use tempfile::{tempdir, TempDir};

    use super::*;

    #[test]
    fn generate_and_load() {
        let dir = tempdir().unwrap();
        let settings = settings(&dir);

        let mut backend = FileSystemKMSBackend::new(settings.clone());
        let key_id = backend
            .register(Either::Left(SupportedKeys::Ed25519))
            .unwrap();
        let public_key = backend.public_key(key_id).unwrap();
        let signature = backend.sign(key_id, Bytes::from("data")).unwrap();

        let mut backend = FileSystemKMSBackend::new(settings);
        assert_eq!(backend.register(Either::Right(key_id)).unwrap(), key_id,);
        assert_eq!(backend.public_key(key_id).unwrap(), public_key);
        assert_eq!(
            backend.sign(key_id, Bytes::from("data")).unwrap(),
            signature,
        );
    }

    #[test]
    fn load_with_wrong_password() {
        let dir = tempdir().unwrap();
        let mut settings = settings(&dir);

        let mut backend = FileSystemKMSBackend::new(settings.clone());
        let key_id = backend
            .register(Either::Left(SupportedKeys::Ed25519))
            .unwrap();

        settings.password = "wrong".to_string();
        let mut backend = FileSystemKMSBackend::new(settings);
        assert!(backend.register(Either::Right(key_id)).is_err());
    }

    #[test]
    fn unregistered_key() {
        let dir = tempdir().unwrap();
        let mut backend = FileSystemKMSBackend::new(settings(&dir));
        let key_id = [0u8; 16];
        assert!(backend.public_key(key_id).is_err());
        assert!(backend.sign(key_id, Bytes::from("data")).is_err());
    }

    fn settings(dir: &TempDir) -> FileSystemKMSBackendSettings {
        FileSystemKMSBackendSettings {
            dir: dir.path().to_path_buf(),
            password: "password".to_string(),
            salt: "salt".to_string(),
            rounds: 1000,
        }
    }
}
