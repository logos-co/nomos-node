// std
// crates
use blake2::Blake2b;
use nomos_core::da::attestation;
use serde::{Deserialize, Serialize};
use sha3::Digest;
// internal

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attestation {
    pub blob_hash: [u8; 32],
    pub signature: Vec<u8>,
}

impl attestation::Attestation for Attestation {
    type Hash = [u8; 32];

    fn blob_hash(&self) -> Self::Hash {
        self.blob_hash
    }

    fn hash(&self) -> Self::Hash {
        Blake2b::new()
            .chain_update(self.blob_hash)
            .finalize()
            .into()
    }

    fn signature(&self) -> &[u8] {
        self.signature.as_ref()
    }
}
