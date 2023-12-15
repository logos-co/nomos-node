// internal
use nomos_core::{
    crypto::PublicKey,
    da::{
        attestation::{self, Attestation as _},
        blob::{self, BlobHasher},
        certificate, DaProtocol,
    },
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
    sender: PublicKey,
    data: Bytes,
}

fn hasher(blob: &Blob) -> [u8; 32] {
    let mut hasher = Blake2bVar::new(32).unwrap();
    hasher.update(&blob.data);
    let mut output = [0; 32];
    hasher.finalize_variable(&mut output).unwrap();
    output
}

impl blob::Blob for Blob {
    const HASHER: BlobHasher<Self> = hasher as BlobHasher<Self>;
    type Hash = [u8; 32];
    type Sender = PublicKey;

    fn as_bytes(&self) -> bytes::Bytes {
        self.data.clone()
    }

    fn sender(&self) -> Self::Sender {
        self.sender.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Attestation {
    blob: [u8; 32],
    voter: Voter,
}

impl attestation::Attestation for Attestation {
    type Blob = Blob;
    type Hash = [u8; 32];

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
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Certificate {
    attestations: Vec<Attestation>,
}

impl Hash for Certificate {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(certificate::Certificate::as_bytes(self).as_ref());
    }
}

impl certificate::Certificate for Certificate {
    type Blob = Blob;
    type Hash = [u8; 32];

    fn blob(&self) -> <Self::Blob as blob::Blob>::Hash {
        self.attestations[0].blob
    }

    fn hash(&self) -> <Self::Blob as blob::Blob>::Hash {
        let mut input = self
            .attestations
            .iter()
            .map(|a| a.hash())
            .collect::<Vec<_>>();
        // sort to make the hash deterministic
        input.sort();
        hash(input.concat())
    }

    fn as_bytes(&self) -> Bytes {
        wire::serialize(self)
            .expect("Certificate shouldn't fail to be serialized")
            .into()
    }
}

// TODO: add generic impl when the trait for Certificate is expanded
impl DaProtocol for FullReplication<AbsoluteNumber<Attestation, Certificate>> {
    type Blob = Blob;
    type Attestation = Attestation;
    type Certificate = Certificate;
    type Settings = Settings;

    fn new(settings: Self::Settings) -> Self {
        Self::new(
            settings.voter,
            AbsoluteNumber::new(settings.num_attestations),
        )
    }

    fn encode<S, T>(&self, sender: S, data: T) -> Vec<Self::Blob>
    where
        T: AsRef<[u8]>,
        S: Into<PublicKey>,
    {
        vec![Blob {
            sender: sender.into(),
            data: Bytes::copy_from_slice(data.as_ref()),
        }]
    }

    fn recv_blob(&mut self, blob: Self::Blob) {
        self.output_buffer.push(blob.data);
    }

    fn extract(&mut self) -> Option<Bytes> {
        self.output_buffer.pop()
    }

    fn attest(&self, blob: &Self::Blob) -> Self::Attestation {
        Attestation {
            blob: hasher(blob),
            voter: self.voter,
        }
    }

    fn validate_attestation(&self, blob: &Self::Blob, attestation: &Self::Attestation) -> bool {
        hasher(blob) == attestation.blob
    }

    fn recv_attestation(&mut self, attestation: Self::Attestation) {
        self.attestations.push(attestation);
        if self.certificate_strategy.can_build(&self.attestations) {
            self.output_certificate_buf.push(
                self.certificate_strategy
                    .build(std::mem::take(&mut self.attestations)),
            );
        }
    }

    fn certify_dispersal(&mut self) -> Option<Self::Certificate> {
        self.output_certificate_buf.pop()
    }

    fn validate_certificate(&self, certificate: &Self::Certificate) -> bool {
        self.certificate_strategy
            .can_build(&certificate.attestations)
    }
}

fn hash(item: impl AsRef<[u8]>) -> [u8; 32] {
    let mut hasher = Blake2bVar::new(32).unwrap();
    hasher.update(item.as_ref());
    let mut output = [0; 32];
    hasher.finalize_variable(&mut output).unwrap();
    output
}
