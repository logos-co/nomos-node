use nomos_mix_message::MessageBuilder;
use rand::SeedableRng;
use rand_chacha::ChaCha12Rng;
use serde::{Deserialize, Serialize};

use crate::{error::Error, membership::Membership};

/// [`CryptographicProcessor`] is responsible for wrapping and unwrapping messages
/// for the message indistinguishability.
pub(crate) struct CryptographicProcessor {
    settings: CryptographicProcessorSettings,
    message_builder: MessageBuilder,
    membership: Membership,
    rng: ChaCha12Rng,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CryptographicProcessorSettings {
    pub num_mix_layers: usize,
    pub max_num_mix_layers: usize,
    pub max_payload_size: usize,
    pub private_key: [u8; 32],
}

impl CryptographicProcessor {
    pub(crate) fn new(
        settings: CryptographicProcessorSettings,
        message_builder: MessageBuilder,
        membership: Membership,
    ) -> Self {
        Self {
            settings,
            message_builder,
            membership,
            rng: ChaCha12Rng::from_entropy(),
        }
    }

    pub(crate) fn wrap_message(&mut self, message: &[u8]) -> Result<Vec<u8>, Error> {
        let public_keys = self.choose_public_keys(self.settings.num_mix_layers);
        if public_keys.len() < self.settings.num_mix_layers {
            return Err(Error::NotSufficientNodes);
        }
        Ok(self.message_builder.new_message(public_keys, message)?)
    }

    pub(crate) fn unwrap_message(&self, message: &[u8]) -> Result<(Vec<u8>, bool), Error> {
        Ok(self
            .message_builder
            .unpack_message(message, self.settings.private_key)?)
    }

    fn choose_public_keys(&mut self, amount: usize) -> Vec<[u8; 32]> {
        self.membership
            .choose_nodes(&mut self.rng, amount)
            .iter()
            .map(|node| node.public_key)
            .collect()
    }
}
