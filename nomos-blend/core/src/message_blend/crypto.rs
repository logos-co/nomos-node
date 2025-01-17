use std::hash::Hash;

use crate::membership::Membership;
use nomos_blend_message::BlendMessage;
use rand::RngCore;
use serde::{Deserialize, Serialize};

/// [`CryptographicProcessor`] is responsible for wrapping and unwrapping messages
/// for the message indistinguishability.
pub struct CryptographicProcessor<R, NodeId, M>
where
    M: BlendMessage,
{
    settings: CryptographicProcessorSettings<M::PrivateKey>,
    membership: Membership<NodeId, M>,
    rng: R,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CryptographicProcessorSettings<K> {
    pub private_key: K,
    pub num_blend_layers: usize,
}

impl<R, NodeId, M> CryptographicProcessor<R, NodeId, M>
where
    R: RngCore,
    M: BlendMessage,
    M::PublicKey: Clone + PartialEq,
    NodeId: Hash + Eq,
{
    pub fn new(
        settings: CryptographicProcessorSettings<M::PrivateKey>,
        membership: Membership<NodeId, M>,
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
            .choose_remote_nodes(&mut self.rng, self.settings.num_blend_layers)
            .iter()
            .map(|node| node.public_key.clone())
            .collect::<Vec<_>>();

        M::build_message(message, &public_keys)
    }

    pub fn unwrap_message(&self, message: &[u8]) -> Result<(Vec<u8>, bool), M::Error> {
        M::unwrap_message(message, &self.settings.private_key)
    }
}
