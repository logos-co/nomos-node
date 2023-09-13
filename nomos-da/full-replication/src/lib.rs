// internal
use nomos_core::da::{
    attestation,
    blob::{self, BlobHasher},
    certificate, DaProtocol,
};
// std
use std::collections::HashSet;
// crates
use blake2::{
    digest::{Update, VariableOutput},
    Blake2bVar,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct FullReplication<CertificateStrategy> {
    certificate_strategy: CertificateStrategy,
    output_buffer: Vec<Bytes>,
    attestations: Vec<Attestation>,
    output_certificate_buf: Vec<Certificate>,
}

impl<S> FullReplication<S> {
    pub fn new(strategy: S) -> Self {
        Self {
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

#[derive(Debug, Clone, Serialize, Deserialize)]

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

impl blob::Blob for Blob {
    type Hash = [u8; 32];
    const HASHER: BlobHasher<Self> = hasher as BlobHasher<Self>;

    fn as_bytes(&self) -> bytes::Bytes {
        self.data.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attestation {
    blob: [u8; 32],
    voter: [u8; 32],
}

impl attestation::Attestation for Attestation {
    type Blob = Blob;
    fn blob(&self) -> [u8; 32] {
        self.blob
    }
    fn as_bytes(&self) -> Bytes {
        Bytes::from([self.blob.as_ref(), self.voter.as_ref()].concat())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Certificate {
    attestations: Vec<Attestation>,
}

impl certificate::Certificate for Certificate {}

// TODO: add generic impl when the trait for Certificate is expanded
impl DaProtocol for FullReplication<AbsoluteNumber<Attestation, Certificate>> {
    type Blob = Blob;
    type Attestation = Attestation;
    type Certificate = Certificate;

    fn encode<T: AsRef<[u8]>>(&self, data: T) -> Vec<Self::Blob> {
        vec![Blob {
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
            // TODO: voter id?
            voter: [0; 32],
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
