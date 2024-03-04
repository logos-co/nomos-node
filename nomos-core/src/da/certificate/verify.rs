use std::marker::PhantomData;

use crate::da::{
    attestation::Attestation,
    auth::Verifier,
    certificate::{Certificate, CertificateVerifier},
};

pub trait KeyStore<K> {
    type Verifier: Verifier;

    fn get_key(&self, node_id: &K) -> Option<&Self::Verifier>;
}

#[derive(Clone, Debug)]
pub struct DaCertificateVerifier<K, KS: KeyStore<K>, C> {
    key_store: KS,
    _cert: PhantomData<C>,
    _key: PhantomData<K>,
}

impl<K, KS: KeyStore<K>, C> DaCertificateVerifier<K, KS, C> {
    pub fn new(key_store: KS) -> Self {
        Self {
            key_store,
            _cert: Default::default(),
            _key: Default::default(),
        }
    }
}

impl<K, KS, C> CertificateVerifier for DaCertificateVerifier<K, KS, C>
where
    C: Certificate + Clone,
    <<C as Certificate>::Attestation as Attestation>::Voter: Into<K> + Clone,
    <<C as Certificate>::Attestation as Attestation>::Hash: AsRef<[u8]>,
    KS: KeyStore<K> + Clone + 'static,
    KS::Verifier: 'static,
{
    type Certificate = C;

    fn verify(&self, certificate: &Self::Certificate) -> bool {
        // TODO: At the moment the node that verifies the certificate needs to have all public
        // keys of voters that attested given certificate.
        certificate.attestations().iter().all(|attestation| {
            self.key_store
                .get_key(&attestation.voter().into())
                .map(|verifier| {
                    verifier.verify(attestation.hash().as_ref(), attestation.signature())
                })
                .unwrap_or(false)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::da::certificate::mock::*;

    #[test]
    fn test_certificate_with_valid_signatures() {
        let mut key_store = MockKeyStore::default();
        key_store.add_key(&[1u8; 32], MockPublicKey);
        key_store.add_key(&[2u8; 32], MockPublicKey);
        key_store.add_key(&[3u8; 32], MockPublicKey);

        let verifier = DaCertificateVerifier::new(key_store);

        let certificate = MockCertificate::new(vec![
            MockAttestation::new(&[1u8; 32], b"valid_signature"),
            MockAttestation::new(&[2u8; 32], b"valid_signature"),
            MockAttestation::new(&[3u8; 32], b"valid_signature"),
        ]);

        assert!(verifier.verify(&certificate));
    }

    #[test]
    fn test_certificate_with_invalid_signature() {
        let mut key_store = MockKeyStore::default();
        key_store.add_key(&[1u8; 32], MockPublicKey);
        key_store.add_key(&[2u8; 32], MockPublicKey);
        key_store.add_key(&[3u8; 32], MockPublicKey);

        let verifier = DaCertificateVerifier::new(key_store);

        let certificate = MockCertificate::new(vec![
            MockAttestation::new(&[1u8; 32], b"invalid_signature"),
            MockAttestation::new(&[2u8; 32], b"valid_signature"),
            MockAttestation::new(&[3u8; 32], b"valid_signature"),
        ]);

        assert!(!verifier.verify(&certificate));
    }

    #[test]
    fn test_certificate_with_missing_pubkey() {
        let mut key_store = MockKeyStore::default();
        key_store.add_key(&[1u8; 32], MockPublicKey);
        key_store.add_key(&[2u8; 32], MockPublicKey);

        let verifier = DaCertificateVerifier::new(key_store);

        let certificate = MockCertificate::new(vec![
            MockAttestation::new(&[1u8; 32], b"valid_signature"),
            MockAttestation::new(&[2u8; 32], b"valid_signature"),
            MockAttestation::new(&[3u8; 32], b"valid_signature"),
        ]);

        assert!(!verifier.verify(&certificate));
    }
}
