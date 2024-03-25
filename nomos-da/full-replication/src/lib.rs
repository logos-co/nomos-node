// internal
use nomos_core::da::{
    attestation::{self, Attestation as _},
    certificate,
};
// std
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
// crates
use blake2::{
    digest::{Update, VariableOutput},
    Blake2bVar,
};
use bytes::Bytes;
use nomos_core::da::certificate_metadata::CertificateExtension;
use nomos_core::wire;
use serde::{Deserialize, Serialize};

/// Re-export the types for OpenAPI
#[cfg(feature = "openapi")]
pub mod openapi {
    pub use super::{Attestation, Certificate};
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

// TODO: maybe abstract in a general library?
trait CertificateStrategy {
    type Attestation: attestation::Attestation;
    type Certificate: certificate::Certificate;

    fn can_build(&self, attestations: &[Self::Attestation]) -> bool;
    fn build(&self, attestations: Vec<Self::Attestation>) -> Certificate;
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

    fn can_build(&self, attestations: &[Self::Attestation]) -> bool {
        attestations.len() >= self.num_attestations
            && attestations
                .iter()
                .map(|a| &a.blob)
                .collect::<HashSet<_>>()
                .len()
                == 1
    }

    fn build(&self, attestations: Vec<Self::Attestation>) -> Certificate {
        assert!(self.can_build(&attestations));
        Certificate { attestations }
    }
}

pub type Voter = [u8; 32];

#[derive(Debug, Clone, Serialize, Deserialize, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Blob {
    data: Bytes,
}

fn hasher(blob: &Blob) -> [u8; 32] {
    let mut hasher = Blake2bVar::new(32).unwrap();
    hasher.update(&blob.data);
    let mut output = [0; 32];
    hasher.finalize_variable(&mut output).unwrap();
    output
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Attestation {
    blob: [u8; 32],
    voter: Voter,
}

impl attestation::Attestation for Attestation {
    type Signature = [u8; 32];

    fn attestation_signature(&self) -> Self::Signature {
        hash([self.blob, self.voter].concat())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Certificate {
    attestations: Vec<Attestation>,
}

impl Hash for Certificate {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(<Certificate as certificate::Certificate>::id(self).as_ref());
    }
}

pub struct CertificateVerificationParameters {
    threshold: usize,
}

impl certificate::Certificate for Certificate {
    type VerificationParameters = CertificateVerificationParameters;
    type Signature = [u8; 32];
    type Id = [u8; 32];

    fn signers(&self) -> Vec<bool> {
        self.attestations.iter().map(|_| true).collect()
    }

    fn signature(&self) -> Self::Signature {
        let signatures: Vec<u8> = self
            .attestations
            .iter()
            .map(|attestation| attestation.attestation_signature())
            .flatten()
            .collect();
        hash(signatures)
    }

    fn id(&self) -> Self::Id {
        let mut input = self
            .attestations
            .iter()
            .map(|a| a.attestation_signature())
            .collect::<Vec<_>>();
        // sort to make the hash deterministic
        input.sort();
        hash(input.concat())
    }

    fn verify(&self, authorization_parameters: Self::VerificationParameters) -> bool {
        authorization_parameters.threshold <= self.attestations.len()
    }
}

impl CertificateExtension for Certificate {
    type Extension = ();

    fn extension(&self) -> Self::Extension {
        ()
    }
}

fn hash(item: impl AsRef<[u8]>) -> [u8; 32] {
    let mut hasher = Blake2bVar::new(32).unwrap();
    hasher.update(item.as_ref());
    let mut output = [0; 32];
    hasher.finalize_variable(&mut output).unwrap();
    output
}
