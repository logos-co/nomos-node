use std::marker::PhantomData;

use crate::da::{attestation::Attestation, auth::Verifier, certificate::Certificate};

use super::CertificateVerifier;

pub trait KeyProvider<K> {
    type Verifier: Verifier;

    fn get_key(&self, node_id: &K) -> Option<&Self::Verifier>;
}

#[derive(Clone, Debug)]
pub struct DaCertificateVerifier<K, KP: KeyProvider<K>, C> {
    key_provider: KP,
    _cert: PhantomData<C>,
    _key: PhantomData<K>,
}

impl<K, KP: KeyProvider<K>, C> DaCertificateVerifier<K, KP, C> {
    pub fn new(key_provider: KP) -> Self {
        Self {
            key_provider,
            _cert: Default::default(),
            _key: Default::default(),
        }
    }
}

impl<K, KP, C> CertificateVerifier for DaCertificateVerifier<K, KP, C>
where
    C: Certificate + Clone,
    <<C as Certificate>::Attestation as Attestation>::Attester: Into<K> + Clone,
    <<C as Certificate>::Attestation as Attestation>::Hash: AsRef<[u8]>,
    KP: KeyProvider<K> + Clone + 'static,
    KP::Verifier: 'static,
{
    type Certificate = C;

    fn verify(&self, certificate: &Self::Certificate) -> bool {
        // TODO: At the moment the node that verifies the certificate needs to have all public
        // keys of voters that attested given certificate.
        certificate.attestations().iter().all(|attestation| {
            self.key_provider
                .get_key(&attestation.attester().into())
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
