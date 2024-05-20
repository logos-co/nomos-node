// std
// crates
use nomos_core::da::attestation;
use serde::{Deserialize, Serialize};
// internal
use crate::common::hash;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attestation {
    pub blob_hash: [u8; 32],
    pub attester: [u8; 32],
    pub signature: Vec<u8>,
}

impl attestation::Attestation for Attestation {
    type Attester = [u8; 32];
    type Hash = [u8; 32];

    fn attester(&self) -> Self::Attester {
        self.attester
    }

    fn blob_hash(&self) -> Self::Hash {
        self.blob_hash
    }

    fn hash(&self) -> Self::Hash {
        hash([self.blob_hash, self.attester].concat())
    }

    fn signature(&self) -> &[u8] {
        self.signature.as_ref()
    }
}
