use std::hash::Hash;

use crate::membership::Membership;
use nomos_mix_message::MixMessage;
use rand::RngCore;
use serde::{Deserialize, Serialize};

/// [`CryptographicProcessor`] is responsible for wrapping and unwrapping messages
/// for the message indistinguishability.
pub struct CryptographicProcessor<R, Address, M>
where
    M: MixMessage,
{
    settings: CryptographicProcessorSettings<M::PrivateKey>,
    membership: Membership<Address, M>,
    rng: R,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CryptographicProcessorSettings<K> {
    pub private_key: K,
    pub num_mix_layers: usize,
}

impl<R, Address, M> CryptographicProcessor<R, Address, M>
where
    R: RngCore,
    Address: Eq + Hash,
    M: MixMessage,
    M::PublicKey: Clone + PartialEq,
{
    pub fn new(
        settings: CryptographicProcessorSettings<M::PrivateKey>,
        membership: Membership<Address, M>,
        rng: R,
    ) -> Self {
        Self {
            settings,
            membership,
            rng,
        }
    }

    pub fn wrap_message(&mut self, message: &[u8]) -> Result<Vec<u8>, M::Error> {
        // TODO: Use the actual Sphinx encoding instead of mock.
        let public_keys = self
            .membership
            .choose_remote_nodes(&mut self.rng, self.settings.num_mix_layers)
            .iter()
            .map(|node| node.public_key.clone())
            .collect::<Vec<_>>();

        M::build_message(message, &public_keys)
    }

    pub fn unwrap_message(&self, message: &[u8]) -> Result<(Vec<u8>, bool), M::Error> {
        M::unwrap_message(message, &self.settings.private_key)
    }
}
