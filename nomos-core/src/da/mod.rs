// crates
use bytes::Bytes;
// internal
use crate::da::attestation::Attestation;
use crate::da::blob::Blob;
use crate::da::certificate::Certificate;

pub mod attestation;
pub mod blob;
pub mod certificate;
pub mod full_replication;

pub trait DaProtocol {
    type Blob: Blob;
    type Attestation: Attestation;
    type Certificate: Certificate;

    /// Encode bytes into blobs
    fn encode<T: AsRef<[u8]>>(&self, data: T) -> Vec<Self::Blob>;
    /// Feed a blob for decoding.
    /// Depending on the protocol, it may be necessary to feed multiple blobs to
    /// recover the initial data.
    fn recv_blob(&mut self, blob: Self::Blob);
    /// Attempt to recover the initial data from feeded blobs.
    /// If the protocol is not yet ready to return the data, return None.
    fn extract(&mut self) -> Option<Bytes>;
    /// Attest that we have received and stored a blob.
    fn attest(&self, blob: &Self::Blob) -> Self::Attestation;
    /// Validate that an attestation is valid for a blob.
    fn validate_attestation(&self, blob: &Self::Blob, attestation: &Self::Attestation) -> bool;
    /// Buffer attestations to produce a certificate of correct dispersal.
    fn recv_attestation(&mut self, attestation: Self::Attestation);
    /// Attempt to produce a certificate of correct disperal for a blob.
    /// If the protocol is not yet ready to return the certificate, return None.
    fn certify_dispersal(&mut self) -> Option<Self::Certificate>;
    /// Validate a certificate.
    fn validate_certificate(certificate: &Self::Certificate) -> bool;
}
