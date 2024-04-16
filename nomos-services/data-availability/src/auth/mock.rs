use std::{fs, path::PathBuf, sync::Arc};

use nomos_core::da::auth::{Signer, Verifier};
use ring::signature::{self, Ed25519KeyPair, KeyPair};
use serde::{Deserialize, Serialize};

use crate::auth::DaAuth;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MockDaAuthSettings {
    pub pkcs8_file_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct MockDaAuth(Arc<Ed25519KeyPair>);

impl DaAuth for MockDaAuth {
    type Settings = MockDaAuthSettings;

    fn new(settings: Self::Settings) -> Self {
        let pkcs8_bytes =
            fs::read(settings.pkcs8_file_path).expect("Failed to read PKCS#8 PEM file");

        let key_pair = Ed25519KeyPair::from_pkcs8_maybe_unchecked(pkcs8_bytes.as_ref())
            .expect("Failed to create Ed25519 key pair from provided PKCS#8 bytes");

        Self(Arc::new(key_pair))
    }
}

impl Signer for MockDaAuth {
    fn sign(&self, message: &[u8]) -> Vec<u8> {
        self.0.sign(message).as_ref().to_vec()
    }
}

impl Verifier for MockDaAuth {
    /// MockDaAuth verifier implementation is used in integration tests.
    /// Certificates should be verified by the CertificateVerifier trait implementor from
    /// nomos_core::da module.
    fn verify(&self, message: &[u8], sig: &[u8]) -> bool {
        let peer_public_key =
            signature::UnparsedPublicKey::new(&signature::ED25519, self.0.public_key());
        peer_public_key.verify(message, sig.as_ref()).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use std::{fs::File, io::Write};

    use ring::{rand, signature};

    #[test]
    #[ignore]
    fn generate_ed25519_key() {
        let rng = rand::SystemRandom::new();
        let pkcs8_bytes = signature::Ed25519KeyPair::generate_pkcs8(&rng).unwrap();

        // Dumps key to `nomos-services/data-availability` dir.
        let mut file = File::create("mock_da_auth_key.p8").expect("Failed to create file");
        file.write_all(pkcs8_bytes.as_ref())
            .expect("Failed to write to file");
    }
}
