use crate::membership::Membership;
use nomos_mix_message::MixMessage;
use rand::RngCore;
use serde::{Deserialize, Serialize};

/// [`CryptographicProcessor`] is responsible for wrapping and unwrapping messages
/// for the message indistinguishability.
pub struct CryptographicProcessor<R, M>
where
    M: MixMessage,
{
    settings: CryptographicProcessorSettings<M::PrivateKey>,
    membership: Membership<M>,
    rng: R,
    mix_message: M,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CryptographicProcessorSettings<K> {
    pub private_key: K,
    pub num_mix_layers: usize,
}

impl<R, M> CryptographicProcessor<R, M>
where
    R: RngCore,
    M: MixMessage,
    M::PublicKey: Clone + PartialEq,
{
    pub fn new(
        settings: CryptographicProcessorSettings<M::PrivateKey>,
        mix_message_settings: M::Settings,
        membership: Membership<M>,
        rng: R,
    ) -> Self {
        let mix_message = M::new(mix_message_settings);
        Self {
            settings,
            membership,
            rng,
            mix_message,
        }
    }

    pub fn wrap_message(&mut self, message: &[u8]) -> Result<Vec<u8>, M::Error> {
        let public_keys = self
            .membership
            .choose_remote_nodes(&mut self.rng, self.settings.num_mix_layers)
            .iter()
            .map(|node| node.public_key.clone())
            .collect::<Vec<_>>();

        self.mix_message.build_message(message, &public_keys)
    }

    pub fn unwrap_message(&self, message: &[u8]) -> Result<(Vec<u8>, bool), M::Error> {
        self.mix_message
            .unwrap_message(message, &self.settings.private_key)
    }
}
