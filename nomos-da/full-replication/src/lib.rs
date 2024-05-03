pub mod attestation;

use attestation::Attestation;
use nomos_core::da::attestation::Attestation as _;
use nomos_core::da::certificate::metadata::Next;
use nomos_core::da::certificate::CertificateStrategy;
// internal
use nomos_core::da::certificate::{self, metadata};
// std
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
// crates
use blake2::{
    digest::{Update, VariableOutput},
    Blake2bVar,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FRIndex(u64);

/// Re-export the types for OpenAPI
#[cfg(feature = "openapi")]
pub mod openapi {
    pub use super::Certificate;
}

#[derive(Debug, Clone)]
pub struct FullReplication<CertificateStrategy> {
    voter: Voter,
    certificate_strategy: CertificateStrategy,
    output_buffer: Vec<Bytes>,
    attestations: Vec<Attestation>,
    output_certificate_buf: Vec<Certificate>,
}

impl<S> FullReplication<S> {
    pub fn new(voter: Voter, strategy: S) -> Self {
        Self {
            voter,
            certificate_strategy: strategy,
            output_buffer: Vec::new(),
            attestations: Vec::new(),
            output_certificate_buf: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AbsoluteNumber<A, C> {
    num_attestations: usize,
    _a: std::marker::PhantomData<A>,
    _c: std::marker::PhantomData<C>,
}

impl<A, C> AbsoluteNumber<A, C> {
    pub fn new(num_attestations: usize) -> Self {
        Self {
            num_attestations,
            _a: std::marker::PhantomData,
            _c: std::marker::PhantomData,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub voter: Voter,
    pub num_attestations: usize,
}

impl CertificateStrategy for AbsoluteNumber<Attestation, Certificate> {
    type Attestation = Attestation;
    type Certificate = Certificate;
    type Metadata = Certificate;

    fn can_build(&self, attestations: &[Self::Attestation]) -> bool {
        attestations.len() >= self.num_attestations
            && attestations
                .iter()
                .map(|a| a.blob_hash())
                .collect::<HashSet<_>>()
                .len()
                == 1
    }

    fn build(
        &self,
        attestations: Vec<Self::Attestation>,
        app_id: [u8; 32],
        index: FRIndex,
    ) -> Certificate {
        assert!(self.can_build(&attestations));
        Certificate {
            attestations,
            metadata: Metadata { app_id, index },
        }
    }
}

pub type Voter = [u8; 32];

#[derive(Debug, Clone, Serialize, Deserialize, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Blob {
    data: Bytes,
}

#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Metadata {
    app_id: [u8; 32],
    index: FRIndex,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Certificate {
    attestations: Vec<Attestation>,
    metadata: Metadata,
}

impl Hash for Certificate {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(<Certificate as certificate::Certificate>::id(self).as_ref());
    }
}

#[derive(Clone, Debug)]
pub struct CertificateVerificationParameters {
    pub threshold: usize,
}

impl certificate::Certificate for Certificate {
    type Id = [u8; 32];
    type Signature = [u8; 32];
    type VerificationParameters = CertificateVerificationParameters;

    fn signature(&self) -> Self::Signature {
        let mut attestations = self.attestations.clone();
        attestations.sort();
        let mut signatures = Vec::new();
        for attestation in &attestations {
            signatures.extend_from_slice(attestation.signature());
        }
        hash(signatures)
    }

    fn id(&self) -> Self::Id {
        let mut input = self
            .attestations
            .iter()
            .map(|a| a.signature())
            .collect::<Vec<_>>();
        // sort to make the hash deterministic
        input.sort();
        hash(input.concat())
    }

    fn signers(&self) -> Vec<bool> {
        unimplemented!()
    }

    fn verify(&self, params: Self::VerificationParameters) -> bool {
        self.attestations.len() >= params.threshold
    }
}

pub struct VID {
    id: [u8; 32],
    metadata: Metadata,
}

impl From<Certificate> for VID {
    fn from(cert: Certificate) -> Self {
        // To simulate the propery of aggregate committment + row commitment in Nomos Da Protocol,
        // when full replication certificate is converted into the VID (which should happen after
        // the verification in the mempool) the id is set to the blob hash to allow identification
        // of the distributed data accross nomos nodes.
        let id = cert.attestations[0].blob_hash();
        Self {
            id,
            metadata: cert.metadata,
        }
    }
}

impl metadata::Metadata for Certificate {
    type AppId = [u8; 32];
    type Index = FRIndex;

    fn metadata(&self) -> (Self::AppId, Self::Index) {
        (self.metadata.app_id, self.metadata.index)
    }
}

impl Next for FRIndex {
    fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

fn hash(item: impl AsRef<[u8]>) -> [u8; 32] {
    let mut hasher = Blake2bVar::new(32).unwrap();
    hasher.update(item.as_ref());
    let mut output = [0; 32];
    hasher.finalize_variable(&mut output).unwrap();
    output
}
