use std::error::Error;
// crates
// internal

pub mod attestation;
pub mod blob;
pub mod certificate;

pub trait DaEncoder {
    type EncodedData;
    fn encode(b: &[u8]) -> Result<Self::EncodedData, impl Error>;
}

pub trait DaVerifier {
    type DaBlob;
    type Attestation;
    type Error;

    fn verify(&self, blob: &Self::DaBlob) -> Result<Self::Attestation, Self::Error>;
}

pub trait DaDispersal {
    type EncodedData;
    type Certificate;

    fn disperse(&self, encoded_data: Self::EncodedData) -> Result<Self::Certificate, impl Error>;
}

pub trait Signer {
    fn sign(&self, message: &[u8]) -> Vec<u8>;
}
