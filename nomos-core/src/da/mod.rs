use std::error::Error;
// crates
// internal

pub mod attestation;
pub mod certificate;
pub mod certificate_metadata;
pub mod vid;

pub trait DaEncoder {
    type EncodedData;
    fn encode(b: &[u8]) -> Result<Self::EncodedData, impl Error>;
}

pub trait DaVerifier {
    type DaBlob;
    type Sk;
    type Pk;
    type Attestation;
    fn verify(
        blob: &Self::DaBlob,
        sk: &Self::Sk,
        nodes_public_keys: &[Self::Pk],
    ) -> Option<Self::Attestation>;
}

pub trait DaDispersal {
    type EncodedData;
    type Certificate;

    fn disperse(&self, encoded_data: Self::EncodedData) -> Result<Self::Certificate, impl Error>;
}
