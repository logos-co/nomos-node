use std::collections::HashMap;

use bytes::Bytes;

use crate::da::{
    attestation::Attestation,
    auth::Verifier,
    blob::Blob,
    certificate::{verify::KeyStore, Certificate},
};

pub struct MockBlob;

impl Blob for MockBlob {
    type Hash = Vec<u8>;
    const HASHER: crate::da::blob::BlobHasher<Self> = unimplemented!();

    fn as_bytes(&self) -> Bytes {
        Bytes::new()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MockAttestation {
    voter: [u8; 32],
    signature: Vec<u8>,
}

impl MockAttestation {
    pub fn new(voter: &[u8; 32], signature: &[u8]) -> Self {
        MockAttestation {
            voter: *voter,
            signature: signature.to_vec(),
        }
    }
}

impl Attestation for MockAttestation {
    type Blob = MockBlob;
    type Hash = Vec<u8>;
    type Voter = [u8; 32];

    fn blob(&self) -> Self::Hash {
        unimplemented!()
    }
    fn hash(&self) -> Self::Hash {
        vec![0u8]
    }
    fn voter(&self) -> Self::Voter {
        self.voter
    }
    fn as_bytes(&self) -> Bytes {
        Bytes::new()
    }
    fn signature(&self) -> &[u8] {
        &self.signature
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MockCertificate {
    attestations: Vec<MockAttestation>,
}

impl MockCertificate {
    pub fn new(attestations: Vec<MockAttestation>) -> Self {
        MockCertificate { attestations }
    }
}

impl Certificate for MockCertificate {
    type Attestation = MockAttestation;
    type Blob = MockBlob;
    type Extension = ();
    type Hash = ();

    fn attestations(&self) -> Vec<Self::Attestation> {
        self.attestations.clone()
    }

    fn as_bytes(&self) -> Bytes {
        Bytes::from_static(&[0u8])
    }

    fn blob(&self) -> <Self::Blob as crate::da::blob::Blob>::Hash {
        unimplemented!()
    }

    fn hash(&self) -> Self::Hash {
        unimplemented!()
    }

    fn extension(&self) -> Self::Extension {
        unimplemented!()
    }
}

#[derive(Clone)]
pub struct MockPublicKey;

impl Verifier for MockPublicKey {
    fn verify(&self, _message: &[u8], sig: &[u8]) -> bool {
        sig == b"valid_signature"
    }
}

#[derive(Clone)]
pub struct MockKeyStore {
    keys: HashMap<[u8; 32], MockPublicKey>,
}

impl Default for MockKeyStore {
    fn default() -> Self {
        Self {
            keys: [([0u8; 32], MockPublicKey)].into(),
        }
    }
}

impl MockKeyStore {
    pub fn add_key(&mut self, voter: &[u8; 32], verifier: MockPublicKey) {
        self.keys.insert(*voter, verifier);
    }
}

impl KeyStore<[u8; 32]> for MockKeyStore {
    type Verifier = MockPublicKey;

    fn get_key(&self, node_id: &[u8; 32]) -> Option<&Self::Verifier> {
        self.keys.get(node_id)
    }
}

#[derive(Default)]
pub struct MockCertVerifier;

impl MockCertVerifier {
    pub fn verify_cert<C>(&self, _cert: &C) -> bool
    where
        C: Certificate,
    {
        true
    }
}
