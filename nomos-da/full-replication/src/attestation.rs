use bytes::Bytes;
use nomos_core::{
    da::{
        attestation::{self, Attestation as _},
        auth::Signer,
        blob,
    },
    wire,
};
use serde::{Deserialize, Serialize};

use crate::{hash, Blob, Voter};

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

    pub fn new_signed<S: Signer>(blob: [u8; 32], voter: Voter, key_pair: &S) -> Self {
        let mut a = Self::new(blob, voter);
        a.sign(key_pair);
        a
    }

    fn sign<S: Signer>(&mut self, key_pair: &S) {
        let attestation_hash = self.hash();
        let signature = key_pair.sign(&attestation_hash);
        self.sig = Some(signature);
    }
}

impl attestation::Attestation for Attestation {
    type Blob = Blob;
    type Hash = [u8; 32];
    type Voter = Voter;

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

    fn signature(&self) -> Option<&[u8]> {
        self.sig.as_deref()
    }
}
