pub mod error;

use error::Error;
use serde::{Deserialize, Serialize};

use crate::MixMessage;

/// A mock implementation of the Sphinx encoding.

const NODE_ID_SIZE: usize = 32;
const PAYLOAD_PADDING_SEPARATOR: u8 = 0x01;
const PAYLOAD_PADDING_SEPARATOR_SIZE: usize = std::mem::size_of::<u8>();

#[derive(Clone, Debug)]
pub struct MockMixMessage {
    settings: MockMixMessageSettings,
    drop_message: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MockMixMessageSettings {
    max_layers: usize,
    max_payload_size: usize,
}

impl MockMixMessageSettings {
    const fn message_size(&self) -> usize {
        NODE_ID_SIZE * self.max_layers + self.max_payload_size + PAYLOAD_PADDING_SEPARATOR_SIZE
    }
}

impl MockMixMessage {
    const fn size(&self) -> usize {
        self.settings.message_size()
    }
}

impl MixMessage for MockMixMessage {
    type PublicKey = [u8; NODE_ID_SIZE];
    type PrivateKey = [u8; NODE_ID_SIZE];
    type Settings = MockMixMessageSettings;
    type Error = Error;

    fn new(settings: Self::Settings) -> Self {
        let drop_message = vec![0; settings.message_size()];
        Self {
            settings,
            drop_message,
        }
    }

    /// The length of the encoded message is fixed to [`MESSAGE_SIZE`] bytes.
    /// The [`MAX_LAYERS`] number of [`NodeId`]s are concatenated in front of the payload.
    /// The payload is zero-padded to the end.
    fn build_message(
        &self,
        payload: &[u8],
        public_keys: &[Self::PublicKey],
    ) -> Result<Vec<u8>, Self::Error> {
        let MockMixMessageSettings {
            max_layers,
            max_payload_size,
        } = self.settings;

        // In this mock, we don't encrypt anything. So, we use public key as just a node ID.
        let node_ids = public_keys;
        if node_ids.is_empty() || node_ids.len() > max_layers {
            return Err(Error::InvalidNumberOfLayers);
        }
        if payload.len() > max_payload_size {
            return Err(Error::PayloadTooLarge);
        }

        let mut message: Vec<u8> = Vec::with_capacity(self.size());

        node_ids.iter().for_each(|node_id| {
            message.extend(node_id);
        });
        // If there is any remaining layers, fill them with zeros.
        (0..max_layers - node_ids.len()).for_each(|_| message.extend(&[0; NODE_ID_SIZE]));

        // Append payload with padding
        message.extend(payload);
        message.push(PAYLOAD_PADDING_SEPARATOR);
        message.extend(std::iter::repeat(0).take(max_payload_size - payload.len()));
        Ok(message)
    }

    fn unwrap_message(
        &self,
        message: &[u8],
        private_key: &Self::PrivateKey,
    ) -> Result<(Vec<u8>, bool), Self::Error> {
        let MockMixMessageSettings { max_layers, .. } = self.settings;

        if message.len() != self.size() {
            return Err(Error::InvalidMixMessage);
        }

        // In this mock, we don't decrypt anything. So, we use private key as just a node ID.
        let node_id = private_key;
        if &message[0..NODE_ID_SIZE] != node_id {
            return Err(Error::MsgUnwrapNotAllowed);
        }

        // If this is the last layer
        if message[NODE_ID_SIZE..NODE_ID_SIZE * 2] == [0; NODE_ID_SIZE] {
            let padded_payload = &message[NODE_ID_SIZE * max_layers..];
            // remove the payload padding
            match padded_payload
                .iter()
                .rposition(|&x| x == PAYLOAD_PADDING_SEPARATOR)
            {
                Some(pos) => {
                    return Ok((padded_payload[0..pos].to_vec(), true));
                }
                _ => return Err(Error::InvalidMixMessage),
            }
        }

        let mut new_message: Vec<u8> = Vec::with_capacity(self.size());
        new_message.extend(&message[NODE_ID_SIZE..NODE_ID_SIZE * max_layers]);
        new_message.extend(&[0; NODE_ID_SIZE]);
        new_message.extend(&message[NODE_ID_SIZE * max_layers..]); // padded payload
        Ok((new_message, false))
    }

    fn drop_message(&self) -> &[u8] {
        &self.drop_message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message() {
        let mock_message = MockMixMessage::new(MockMixMessageSettings {
            max_layers: 5,
            max_payload_size: 100,
        });

        let node_ids = (0..3).map(|i| [i; NODE_ID_SIZE]).collect::<Vec<_>>();
        let payload = [7; 10];
        let message = mock_message.build_message(&payload, &node_ids).unwrap();
        assert_eq!(message.len(), mock_message.size());

        let (message, is_fully_unwrapped) =
            mock_message.unwrap_message(&message, &node_ids[0]).unwrap();
        assert!(!is_fully_unwrapped);
        assert_eq!(message.len(), mock_message.size());

        let (message, is_fully_unwrapped) =
            mock_message.unwrap_message(&message, &node_ids[1]).unwrap();
        assert!(!is_fully_unwrapped);
        assert_eq!(message.len(), mock_message.size());

        let (unwrapped_payload, is_fully_unwrapped) =
            mock_message.unwrap_message(&message, &node_ids[2]).unwrap();
        assert!(is_fully_unwrapped);
        assert_eq!(unwrapped_payload, payload);
    }
}
