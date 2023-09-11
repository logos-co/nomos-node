// std
use std::error::Error;
// crates
use bytes::Bytes;
use futures::Stream;
// internal
use crate::da::attestation::Attestation;
use crate::da::blob::Blob;
use crate::da::certificate::Certificate;

pub mod attestation;
pub mod blob;
pub mod certificate;

pub trait DaProtocol {
    type Blob: Blob;
    type Attestation: Attestation;
    type Certificate: Certificate;

    fn encode<T: AsRef<[u8]>>(&self, data: T) -> Box<dyn Stream<Item = Self::Blob>>;
    fn decode<S: Stream<Item = Self::Blob>>(&self, s: S) -> Result<Bytes, Box<dyn Error>>;
    fn validate_attestation(&self, blob: &Self::Blob, attestation: &Self::Attestation) -> bool;

    fn certificate_dispersal<S: Stream<Item = Self::Attestation>>(
        &self,
        attestations: S,
    ) -> Self::Certificate;

    fn validate_certificate(certificate: &Self::Certificate) -> bool;
}
