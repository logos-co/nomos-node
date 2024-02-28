use std::marker::PhantomData;

use crate::da::{
    attestation::Attestation,
    auth::Verifier,
    certificate::{Certificate, CertificateVerifier},
};

pub trait KeyStore {
    type Key;
    type Verifier: Verifier;

    fn get_key(&self, node_id: &Self::Key) -> Option<&Self::Verifier>;
}

pub struct DaCertificateVerifier<KS: KeyStore, C> {
    key_store: KS,
    _cert: PhantomData<C>,
}

impl<KS: KeyStore, C> DaCertificateVerifier<KS, C> {
    pub fn new(key_store: KS) -> Self {
        Self {
            key_store,
            _cert: Default::default(),
        }
    }
}

impl<KS, C> CertificateVerifier for DaCertificateVerifier<KS, C>
where
    C: Certificate + Clone,
    <<C as Certificate>::Attestation as Attestation>::Voter: Clone,
    KS: KeyStore<Key = <C::Attestation as Attestation>::Voter> + Default + Clone + 'static,
    KS::Verifier: 'static,
{
    type Certificate = C;

    fn verify(&self, certificate: &Self::Certificate) -> bool {
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
    use crate::da::certificate::mock::*;

    #[test]
    fn test_certificate_with_valid_signatures() {
        let mut key_store = MockKeyStore::default();
        key_store.add_key("attester1", MockPublicKey);
        key_store.add_key("attester2", MockPublicKey);
        key_store.add_key("attester3", MockPublicKey);

        let verifier = DaCertificateVerifier::new(key_store);

        let certificate = MockCertificate::new(vec![
            MockAttestation::new("attester1", b"valid_signature"),
            MockAttestation::new("attester2", b"valid_signature"),
            MockAttestation::new("attester3", b"valid_signature"),
        ]);

        assert!(verifier.verify(&certificate));
    }

    #[test]
    fn test_certificate_with_invalid_signature() {
        let mut key_store = MockKeyStore::default();
        key_store.add_key("attester1", MockPublicKey);
        key_store.add_key("attester2", MockPublicKey);
        key_store.add_key("attester3", MockPublicKey);

        let verifier = DaCertificateVerifier::new(key_store);

        let certificate = MockCertificate::new(vec![
            MockAttestation::new("attester1", b"invalid_signature"),
            MockAttestation::new("attester2", b"valid_signature"),
            MockAttestation::new("attester3", b"valid_signature"),
        ]);

        assert!(!verifier.verify(&certificate));
    }
}
