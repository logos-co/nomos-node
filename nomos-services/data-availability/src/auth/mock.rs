use nomos_core::da::auth::Signer;
use ring::{rand, signature::Ed25519KeyPair};
use std::sync::Arc;

use super::DaAuth;

#[derive(Clone)]
pub struct MockDaAuth(Arc<Ed25519KeyPair>);

impl Default for MockDaAuth {
    fn default() -> Self {
        let rng = rand::SystemRandom::new();
        let pkcs8_bytes =
            Ed25519KeyPair::generate_pkcs8(&rng).expect("Failed to generate PKCS8 for Ed25519");
        let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes.as_ref())
            .expect("Failed to generate Ed25519 key pair");

        Self(Arc::new(key_pair))
    }
}

impl DaAuth for MockDaAuth {
    type Settings = ();

    fn new(_settings: Self::Settings) -> Self {
        Self::default()
    }
}

impl Signer for MockDaAuth {
    fn sign(&self, message: &[u8]) -> Vec<u8> {
        self.0.sign(message).as_ref().to_vec()
    }
}
