use crate::da::attestation::Attestation;
use crate::da::blob::Blob;
use bytes::Bytes;
use futures::Stream;
use serde::de::Error;

pub mod attestation;
pub mod blob;

pub trait DaProtocol {
    type Blob: Blob;
    type Attestation: Attestation;

    fn encode<T: AsRef<[u8]>>(&self, data: T) -> Box<dyn Stream<Item = Self::Blob>>;
    fn decode<S: Stream<Item = Self::Blob>>(&self, s: S) -> Result<Bytes, Box<dyn Error>>;
    fn validate(&self, blob: &Self::Blob, attestation: &Self::Attestation) -> bool;
}
