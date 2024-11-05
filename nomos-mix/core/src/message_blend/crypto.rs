use nomos_mix_message::{new_message, unwrap_message};
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::membership::Membership;

/// [`CryptographicProcessor`] is responsible for wrapping and unwrapping messages
/// for the message indistinguishability.
#[derive(Clone, Debug)]
pub struct CryptographicProcessor<R> {
    settings: CryptographicProcessorSettings,
    membership: Membership,
    rng: R,
    public_key: [u8; 32],
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct CryptographicProcessorSettings {
    pub private_key: [u8; 32],
    pub num_mix_layers: usize,
}

impl<R: Rng> CryptographicProcessor<R> {
    pub fn new(settings: CryptographicProcessorSettings, membership: Membership, rng: R) -> Self {
        let public_key =
            x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(settings.private_key))
                .to_bytes();
        Self {
            settings,
            membership,
            rng,
            public_key,
        }
    }

    pub fn wrap_message(&mut self, message: &[u8]) -> Result<Vec<u8>, nomos_mix_message::Error> {
        // TODO: Use the actual Sphinx encoding instead of mock.
        let node_ids = self
            .membership
            .choose_remote_nodes(&mut self.rng, self.settings.num_mix_layers)
            .iter()
            .map(|node| node.public_key)
            .collect::<Vec<_>>();
        new_message(message, &node_ids)
    }

    pub fn unwrap_message(
        &self,
        message: &[u8],
    ) -> Result<(Vec<u8>, bool), nomos_mix_message::Error> {
        unwrap_message(message, &self.public_key)
    }
}
