use crate::da::{attestation::Attestation, auth::Verifier, certificate::Certificate};

pub trait KeyStore {
    type Key;
    type Verifier: Verifier;

    fn get_key(&self, node_id: &Self::Key) -> Option<&Self::Verifier>;
}

pub struct CertificateVerifier<KS: KeyStore> {
    key_store: KS,
}

impl<KS: KeyStore> CertificateVerifier<KS> {
    pub fn new(key_store: KS) -> Self {
        CertificateVerifier { key_store }
    }

    pub fn verify_certificate<C>(&self, certificate: &C) -> bool
    where
        C: Certificate,
        KS: KeyStore<Key = <C::Attestation as Attestation>::Voter> + Clone + 'static,
    {
        // TODO: At the moment the node that verifies the certificate needs to have all public
        // keys of voters that attested given certificate.
        certificate.attestations().iter().all(|attestation| {
            self.key_store
                .get_key(&attestation.voter())
                .and_then(|verifier| {
                    attestation.signature().map(|signature| {
                        verifier.verify(&attestation.as_bytes(), signature.as_ref())
                    })
                })
                .unwrap_or(false)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::da::{blob::Blob, certificate::Certificate};
    use bytes::Bytes;
    use std::collections::HashMap;

    struct MockBlob;

    impl Blob for MockBlob {
        type Hash = Vec<u8>;
        const HASHER: crate::da::blob::BlobHasher<Self> = unimplemented!();

        fn as_bytes(&self) -> Bytes {
            Bytes::new()
        }
    }

    #[derive(Clone, Debug, PartialEq)]
    struct MockAttestation {
        voter: String,
        signature: Vec<u8>,
    }

    impl MockAttestation {
        fn new(voter: &str, signature: &[u8]) -> Self {
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
        fn signature(&self) -> Option<&[u8]> {
            Some(&self.signature)
        }
    }

    #[derive(Clone, Debug, PartialEq)]
    struct MockCertificate {
        attestations: Vec<MockAttestation>,
    }

    impl MockCertificate {
        fn new(attestations: Vec<MockAttestation>) -> Self {
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
    struct MockPublicKey;

    impl Verifier for MockPublicKey {
        fn verify(&self, _message: &[u8], sig: &[u8]) -> bool {
            sig == b"valid_signature"
        }
    }

    #[derive(Default, Clone)]
    struct MockKeyStore {
        keys: HashMap<String, MockPublicKey>,
    }

    impl MockKeyStore {
        fn add_key(&mut self, voter: &str, verifier: MockPublicKey) {
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

    #[test]
    fn test_certificate_with_valid_signatures() {
        let mut key_store = MockKeyStore::default();
        key_store.add_key("attester1", MockPublicKey);
        key_store.add_key("attester2", MockPublicKey);
        key_store.add_key("attester3", MockPublicKey);

        let verifier = CertificateVerifier::new(key_store);

        let certificate = MockCertificate::new(vec![
            MockAttestation::new("attester1", b"valid_signature"),
            MockAttestation::new("attester2", b"valid_signature"),
            MockAttestation::new("attester3", b"valid_signature"),
        ]);

        assert!(verifier.verify_certificate(&certificate));
    }

    #[test]
    fn test_certificate_with_invalid_signature() {
        let mut key_store = MockKeyStore::default();
        key_store.add_key("attester1", MockPublicKey);
        key_store.add_key("attester2", MockPublicKey);
        key_store.add_key("attester3", MockPublicKey);

        let verifier = CertificateVerifier::new(key_store);

        let certificate = MockCertificate::new(vec![
            MockAttestation::new("attester1", b"invalid_signature"),
            MockAttestation::new("attester2", b"valid_signature"),
            MockAttestation::new("attester3", b"valid_signature"),
        ]);

        assert!(!verifier.verify_certificate(&certificate));
    }
}
