use bytes::Bytes;
use nomos_core::{
    da::{attestation, auth::Signer, blob},
    wire,
};
use serde::{Deserialize, Serialize};

use crate::{hash, Blob, Voter};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Attestation {
    blob: [u8; 32],
    voter: Voter,
    sig: Vec<u8>,
}

impl Attestation {
    pub fn new_signed<S: Signer>(blob: [u8; 32], voter: Voter, key_pair: &S) -> Self {
        let attestation_hash = hash([blob, voter].concat());
        let sig = key_pair.sign(&attestation_hash);
        Self { blob, voter, sig }
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

    fn signature(&self) -> &[u8] {
        self.sig.as_ref()
    }
}
