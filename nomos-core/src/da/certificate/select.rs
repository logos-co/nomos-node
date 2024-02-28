// std
use std::marker::PhantomData;
// crates

use crate::da::attestation::Attestation;
use crate::da::auth::Verifier;
// internal
use crate::da::certificate::{BlobCertificateSelect, Certificate};
use crate::utils;

#[derive(Default, Clone, Copy)]
pub struct FillSize<const SIZE: usize, B> {
    _blob: PhantomData<B>,
}

impl<const SIZE: usize, B> FillSize<SIZE, B> {
    pub fn new() -> Self {
        Self {
            _blob: Default::default(),
        }
    }
}

impl<const SIZE: usize, C: Certificate> BlobCertificateSelect for FillSize<SIZE, C> {
    type Certificate = C;
    type Settings = ();

    fn new(_settings: Self::Settings) -> Self {
        FillSize::new()
    }

    fn select_blob_from<'i, I: Iterator<Item = Self::Certificate> + 'i>(
        &self,
        certificates: I,
    ) -> impl Iterator<Item = Self::Certificate> + 'i {
        utils::select::select_from_till_fill_size::<SIZE, Self::Certificate>(
            |blob| blob.as_bytes().len(),
            certificates,
        )
    }
}

pub trait KeyStore {
    type Key;
    type Verifier: Verifier;

    fn get_key(&self, node_id: &Self::Key) -> Option<&Self::Verifier>;
}

#[derive(Default, Clone, Copy)]
pub struct FillVerifiedSize<const SIZE: usize, C, KS: KeyStore> {
    key_store: KS,
    _cert: PhantomData<C>,
}

impl<const SIZE: usize, C, KS> FillVerifiedSize<SIZE, C, KS>
where
    KS: KeyStore,
    KS::Key: Clone,
{
    pub fn new(key_store: KS) -> Self {
        Self {
            key_store,
            _cert: Default::default(),
        }
    }
}

impl<const SIZE: usize, C, KS> BlobCertificateSelect for FillVerifiedSize<SIZE, C, KS>
where
    C: Certificate + Clone,
    <<C as Certificate>::Attestation as Attestation>::Voter: Clone,
    KS: KeyStore<Key = <C::Attestation as Attestation>::Voter> + Default + Clone + 'static,
    KS::Verifier: 'static,
{
    type Certificate = C;
    type Settings = ();

    fn new(_settings: Self::Settings) -> Self {
        FillVerifiedSize::new(KS::default())
    }

    fn select_blob_from<'i, I: Iterator<Item = Self::Certificate> + 'i>(
        &self,
        certificates: I,
    ) -> impl Iterator<Item = Self::Certificate> + 'i {
        let key_store = self.key_store.clone();
        let mut current_size = 0usize;

        certificates.filter_map(move |certificate| {
            if certificate.attestations().iter().any(|attestation| {
                key_store
                    .get_key(&attestation.voter())
                    .and_then(|voter_key| {
                        attestation
                            .signature()
                            .map(|signature| (voter_key, signature))
                    })
                    .map_or(false, |(voter_key, signature)| {
                        voter_key.verify(&attestation.as_bytes(), signature)
                    })
            }) {
                let cert_size = certificate.as_bytes().len();
                if current_size + cert_size <= SIZE {
                    current_size += cert_size;
                    Some(certificate)
                } else {
                    None
                }
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::da::blob::Blob;
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
        signature: String,
    }

    impl MockAttestation {
        fn new(voter: &str, signature: &str) -> Self {
            MockAttestation {
                voter: voter.to_string(),
                signature: signature.to_string(),
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
            Some(self.signature.as_bytes())
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
    fn test_valid_signature() {
        let mut key_store = MockKeyStore::default();
        key_store.add_key("attester1", MockPublicKey);

        let fill_verified_size =
            FillVerifiedSize::<1, MockCertificate, MockKeyStore>::new(key_store);

        let certificate =
            MockCertificate::new(vec![MockAttestation::new("attester1", "valid_signature")]);

        let certificates = vec![certificate.clone()];
        let result: Vec<_> = fill_verified_size
            .select_blob_from(certificates.into_iter())
            .collect();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], certificate);
    }

    #[test]
    fn test_invalid_signature() {
        let mut key_store = MockKeyStore::default();
        key_store.add_key("attester1", MockPublicKey);

        let fill_verified_size =
            FillVerifiedSize::<1, MockCertificate, MockKeyStore>::new(key_store);

        let certificate =
            MockCertificate::new(vec![MockAttestation::new("attester1", "invalid_signature")]);

        let certificates = vec![certificate];
        let result: Vec<_> = fill_verified_size
            .select_blob_from(certificates.into_iter())
            .collect();

        assert!(result.is_empty());
    }

    #[test]
    fn test_size_filled() {
        let mut key_store = MockKeyStore::default();
        key_store.add_key("attester1", MockPublicKey);
        key_store.add_key("attester2", MockPublicKey);
        key_store.add_key("attester3", MockPublicKey);
        key_store.add_key("attester4", MockPublicKey);
        key_store.add_key("attester5", MockPublicKey);

        let fill_verified_size =
            FillVerifiedSize::<3, MockCertificate, MockKeyStore>::new(key_store);

        let certificates = vec![
            MockCertificate::new(vec![MockAttestation::new("attester1", "invalid_signature")]),
            MockCertificate::new(vec![MockAttestation::new("attester2", "valid_signature")]),
            MockCertificate::new(vec![MockAttestation::new("attester3", "valid_signature")]),
            MockCertificate::new(vec![MockAttestation::new("attester4", "valid_signature")]),
            MockCertificate::new(vec![MockAttestation::new("attester5", "valid_signature")]),
        ];

        let result: Vec<_> = fill_verified_size
            .select_blob_from(certificates.into_iter())
            .collect();

        assert_eq!(result.len(), 3);
    }
}
