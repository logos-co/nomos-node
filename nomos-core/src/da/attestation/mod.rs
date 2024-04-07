pub trait Attestation {
    type Signature;
    fn attestation_signature(&self) -> Self::Signature;
}
