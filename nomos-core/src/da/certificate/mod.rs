pub mod metadata;
pub mod mock;
pub mod select;
pub mod vid;

pub trait Certificate {
    type Signature;
    type Id;
    type VerificationParameters;

    fn signers(&self) -> Vec<bool>;
    fn signature(&self) -> Self::Signature;
    fn id(&self) -> Self::Id;
    fn verify(&self, verification_params: Self::VerificationParameters) -> bool;
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
