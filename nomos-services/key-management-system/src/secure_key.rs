use bytes::Bytes;
use overwatch_rs::DynError;

pub trait SecuredKey {
    fn sign(&self) -> Result<Bytes, DynError>;
    fn as_pk(&self) -> Bytes;
}