use bytes::Bytes;
use nomos_core::{
    da::{
        attestation::{self, Attestation as _},
        auth::Signer as _,
        blob,
    },
    wire,
};
use serde::{Deserialize, Serialize};

use crate::{auth::FullReplicationKeyPair, hash, Blob, Voter};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Attestation {
    blob: [u8; 32],
    voter: Voter,
    sig: Option<Vec<u8>>,
}

impl Attestation {
    pub fn new(blob: [u8; 32], voter: Voter) -> Self {
        Self {
            blob,
            voter,
            sig: None,
        }
    }

    pub fn new_signed(blob: [u8; 32], voter: Voter, key_pair: &FullReplicationKeyPair) -> Self {
        let mut a = Self::new(blob, voter);
        a.sign(key_pair);
        a
    }

    fn sign(&mut self, key_pair: &FullReplicationKeyPair) {
        let attestation_hash = self.hash();
        let signature = key_pair.sign(&attestation_hash);
        self.sig = Some(signature);
    }
}

impl attestation::Attestation for Attestation {
    type Blob = Blob;
    type Hash = [u8; 32];
    type Voter = Voter;
    type Signature = Vec<u8>;

    fn blob(&self) -> [u8; 32] {
        self.blob
    }

    fn hash(&self) -> <Self::Blob as blob::Blob>::Hash {
        hash([self.blob, self.voter].concat())
    }

    fn as_bytes(&self) -> Bytes {
        wire::serialize(self)
            .expect("Attestation shouldn't fail to be serialized")
            .into()
    }

    fn voter(&self) -> Self::Voter {
        self.voter
    }

    fn signature(&self) -> Option<Self::Signature> {
        self.sig.clone()
    }
}
