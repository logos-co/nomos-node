use bytes::Bytes;
use overwatch::DynError;

pub trait SecuredKey {
    fn sign(&self, data: Bytes) -> Result<Bytes, DynError>;
    fn as_pk(&self) -> Bytes;
}
