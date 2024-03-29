use crate::da::{attestation::Attestation, certificate::Certificate};

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
    type Hash = Vec<u8>;
    type Attester = [u8; 32];

    fn blob_hash(&self) -> Self::Hash {
        unimplemented!()
    }
    fn hash(&self) -> Self::Hash {
        vec![0u8]
    }
    fn attester(&self) -> Self::Attester {
        self.voter
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
    type Signature = [u8; 32];
    type Id = [u8; 32];
    type VerificationParameters = ();

    fn signers(&self) -> Vec<bool> {
        todo!()
    }

    fn signature(&self) -> Self::Signature {
        todo!()
    }

    fn id(&self) -> Self::Id {
        todo!()
    }

    fn verify(&self, _: Self::VerificationParameters) -> bool {
        todo!()
    }

    fn size(&self) -> usize {
        todo!()
    }
}
