pub mod select;

pub trait Certificate {
    type VerificationParameters;
    type Signature;
    type Id;
    fn signers(&self) -> Vec<bool>;
    fn signature(&self) -> Self::Signature;
    fn id(&self) -> Self::Id;
    fn verify(&self, authorization_parameters: Self::VerificationParameters) -> bool;
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
