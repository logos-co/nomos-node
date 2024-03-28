use bytes::Bytes;
use std::hash::Hash;

pub trait Attestation {
    type Signature;
    fn attestation_signature(&self) -> Self::Signature;
}
