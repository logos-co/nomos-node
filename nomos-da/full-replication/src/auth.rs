use nomos_core::da::auth::Signer;
use ring::signature::Ed25519KeyPair;

pub struct FullReplicationKeyPair(Ed25519KeyPair);

impl FullReplicationKeyPair {
    pub fn new(key_pair: Ed25519KeyPair) -> Self {
        Self(key_pair)
    }
}

impl Signer for FullReplicationKeyPair {
    fn sign(&self, message: &[u8]) -> Vec<u8> {
        self.0.sign(message).as_ref().to_vec()
    }
}

impl From<Ed25519KeyPair> for FullReplicationKeyPair {
    fn from(key_pair: Ed25519KeyPair) -> Self {
        FullReplicationKeyPair::new(key_pair)
    }
}
