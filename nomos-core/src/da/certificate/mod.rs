pub mod metadata;
pub mod mock;
pub mod select;
pub mod verify;
pub mod vid;

use super::attestation::Attestation;

pub trait Certificate {
    type Attestation: Attestation;
    type Signature;
    type Id;

    fn signers(&self) -> Vec<bool>;
    fn signature(&self) -> Self::Signature;
    fn attestations(&self) -> Vec<Self::Attestation>;
    fn id(&self) -> Self::Id;
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
    type Certificate;
    type Metadata: metadata::Metadata;

    fn can_build(&self, attestations: &[Self::Attestation]) -> bool;
    fn build(
        &self,
        attestations: Vec<Self::Attestation>,
        app_id: <Self::Metadata as metadata::Metadata>::AppId,
        index: <Self::Metadata as metadata::Metadata>::Index,
    ) -> Self::Certificate;
}

pub trait CertificateVerifier {
    type Certificate: Certificate;

    fn verify(&self, certificate: &Self::Certificate) -> bool;
}
