pub mod blob;

use blob::Blob;

pub type BlobId = [u8; 32];

pub trait DaEncoder {
    type EncodedData;
    type Error;

    fn encode(&self, b: &[u8]) -> Result<Self::EncodedData, Self::Error>;
}

pub trait DaVerifier {
    type DaBlob: Blob;
    type Error;

    fn verify(&self, blob: &Self::DaBlob) -> Result<(), Self::Error>;
}

#[async_trait::async_trait]
pub trait DaDispersal {
    type EncodedData;
    type Error;

    async fn disperse(&self, encoded_data: Self::EncodedData) -> Result<(), Self::Error>;
}
