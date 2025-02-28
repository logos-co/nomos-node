#[cfg(feature = "preload")]
pub mod preload;

use bytes::Bytes;
use overwatch_rs::DynError;

use crate::KMSOperator;

#[async_trait::async_trait]
pub trait KMSBackend {
    type SupportedKeyTypes;
    type KeyId;
    type Settings;

    fn new(settings: Self::Settings) -> Self;
    fn register(
        &mut self,
        key_id: Self::KeyId,
        key_scheme: Self::SupportedKeyTypes,
    ) -> Result<Self::KeyId, DynError>;
    fn public_key(&self, key_id: Self::KeyId) -> Result<Bytes, DynError>;
    fn sign(&self, key_id: Self::KeyId, data: Bytes) -> Result<Bytes, DynError>;
    async fn execute(&mut self, key_id: Self::KeyId, mut op: KMSOperator) -> Result<(), DynError>;
}
