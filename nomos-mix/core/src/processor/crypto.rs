use nomos_mix_message::{new_message, unwrap_message};
use serde::{Deserialize, Serialize};

/// [`CryptographicProcessor`] is responsible for wrapping and unwrapping messages
/// for the message indistinguishability.
pub struct CryptographicProcessor {
    settings: CryptographicProcessorSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CryptographicProcessorSettings {
    pub num_mix_layers: usize,
}

impl Default for CryptographicProcessorSettings {
    fn default() -> Self {
        Self { num_mix_layers: 1 }
    }
}

impl CryptographicProcessor {
    pub fn new(settings: CryptographicProcessorSettings) -> Self {
        Self { settings }
    }

    pub fn wrap_message(&self, message: &[u8]) -> Result<Vec<u8>, nomos_mix_message::Error> {
        // TODO: Use the actual Sphinx encoding instead of mock.
        // TODO: Select `num_mix_layers` random nodes from the membership.
        new_message(message, self.settings.num_mix_layers.try_into().unwrap())
    }

    pub fn unwrap_message(
        &self,
        message: &[u8],
    ) -> Result<(Vec<u8>, bool), nomos_mix_message::Error> {
        unwrap_message(message)
    }
}
