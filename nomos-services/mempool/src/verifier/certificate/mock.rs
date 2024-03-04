use nomos_core::da::attestation::Attestation;
use nomos_core::da::certificate::mock::MockKeyStore;
use nomos_core::da::certificate::verify::{DaCertificateVerifier, KeyProvider};
use nomos_core::da::certificate::{Certificate, CertificateVerifier};
use nomos_da::auth::mock::{MockDaAuth, MockDaAuthSettings};
use nomos_da::auth::DaAuth;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::verifier::Verifier;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MockDaVerifierSettings {
    pub node_keys: Vec<([u8; 32], PathBuf)>,
}

impl<K, C> Verifier<C> for DaCertificateVerifier<K, MockKeyStore<MockDaAuth>, C>
where
    C: Certificate + Clone,
    <<C as Certificate>::Attestation as Attestation>::Voter: Into<K> + Clone,
    <<C as Certificate>::Attestation as Attestation>::Hash: AsRef<[u8]>,
    MockKeyStore<MockDaAuth>: KeyProvider<K> + 'static,
    <MockKeyStore<MockDaAuth> as KeyProvider<K>>::Verifier: 'static,
{
    type Settings = MockDaVerifierSettings;

    fn new(settings: Self::Settings) -> Self {
        // TODO: Mempool needs to verify that certificates are composed of attestations signed by
        // valid da nodes. To verify that, node needs to maintain a list of DA Node public keys. At
        // the moment we are using mock key store, which implements KeyProvider trait. The main
        // functionality of this trait is to get public key which could be used for signature
        // verification. In the future public key retrieval might be implemented as a seperate
        // Overwatch service.
        let mut store = MockKeyStore::default();
        for (node, key_path) in settings.node_keys {
            let key = <MockDaAuth as DaAuth>::new(MockDaAuthSettings {
                pkcs8_file_path: key_path,
            });
            store.add_key(&node, key);
        }
        DaCertificateVerifier::new(store)
    }

    fn verify(&self, item: &C) -> bool {
        CertificateVerifier::verify(self, item)
    }
}
