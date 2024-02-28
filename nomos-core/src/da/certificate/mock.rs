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
    voter: String,
    signature: Vec<u8>,
}

impl MockAttestation {
    pub fn new(voter: &str, signature: &[u8]) -> Self {
        MockAttestation {
            voter: voter.to_string(),
            signature: signature.to_vec(),
        }
    }
}

impl Attestation for MockAttestation {
    type Blob = MockBlob;
    type Hash = Vec<u8>;
    type Voter = String;

    fn blob(&self) -> Self::Hash {
        unimplemented!()
    }
    fn hash(&self) -> Self::Hash {
        unimplemented!()
    }
    fn voter(&self) -> Self::Voter {
        self.voter.clone()
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

#[derive(Default, Clone)]
pub struct MockKeyStore {
    keys: HashMap<String, MockPublicKey>,
}

impl MockKeyStore {
    pub fn add_key(&mut self, voter: &str, verifier: MockPublicKey) {
        self.keys.insert(voter.to_string(), verifier);
    }
}

impl KeyStore for MockKeyStore {
    type Key = String;
    type Verifier = MockPublicKey;

    fn get_key(&self, node_id: &Self::Key) -> Option<&Self::Verifier> {
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
