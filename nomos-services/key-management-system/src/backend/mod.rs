pub mod fs;

use crate::KMSOperator;
use bytes::Bytes;
use either::Either;
use overwatch_rs::DynError;

#[async_trait::async_trait]
pub trait KMSBackend {
    type SupportedKeys;
    type KeyId;
    type Settings;

    fn new(settings: Self::Settings) -> Self;
    fn register(
        &mut self,
        key: Either<Self::SupportedKeys, Self::KeyId>,
    ) -> Result<Self::KeyId, DynError>;
    fn public_key(&self, key_id: Self::KeyId) -> Result<Bytes, DynError>;
    fn sign(&mut self, key_id: Self::KeyId, data: Bytes) -> Result<Bytes, DynError>;
    async fn execute(&mut self, key_id: Self::KeyId, mut op: KMSOperator) -> Result<(), DynError>;
}
