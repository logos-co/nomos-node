pub mod select;
pub mod verify;

use crate::da::blob::Blob;
use bytes::Bytes;
use std::hash::Hash;

use super::attestation::Attestation;

pub trait Certificate {
    type Blob: Blob;
    type Attestation: Attestation;
    type Extension;
    type Hash: Hash + Eq + Clone;
    fn blob(&self) -> <Self::Blob as Blob>::Hash;
    fn attestations(&self) -> Vec<Self::Attestation>;
    fn hash(&self) -> Self::Hash;
    fn extension(&self) -> Self::Extension;
    fn as_bytes(&self) -> Bytes;
}

pub trait BlobCertificateSelect {
    type Certificate: Certificate;
    type Settings: Clone;

    fn new(settings: Self::Settings) -> Self;
    fn select_blob_from<'i, I: Iterator<Item = Self::Certificate> + 'i>(
        &self,
        certificates: I,
    ) -> impl Iterator<Item = Self::Certificate> + 'i;
}

pub trait CertificateStrategy {
    type Attestation;
    type Certificate: Certificate;

    fn can_build(&self, attestations: &[Self::Attestation]) -> bool;
    fn build(
        &self,
        attestations: Vec<Self::Attestation>,
        extension: <Self::Certificate as Certificate>::Extension,
    ) -> Self::Certificate;
}
