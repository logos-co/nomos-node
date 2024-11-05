use crate::{Error, MixMessage};
// TODO: Remove all the mock below once the actual implementation is integrated to the system.
//
/// A mock implementation of the Sphinx encoding.

pub type NodeId = [u8; NODE_ID_SIZE];
const NODE_ID_SIZE: usize = 32;
const DUMMY_NODE_ID: NodeId = [0; NODE_ID_SIZE];

const PADDED_PAYLOAD_SIZE: usize = 2048;
const PAYLOAD_PADDING_SEPARATOR: u8 = 0x01;
const PAYLOAD_PADDING_SEPARATOR_SIZE: usize = 1;
const MAX_LAYERS: usize = 5;
pub const MESSAGE_SIZE: usize = NODE_ID_SIZE * MAX_LAYERS + PADDED_PAYLOAD_SIZE;

#[derive(Clone, Debug)]
pub struct MockMixMessage;

impl MixMessage for MockMixMessage {
    type PublicKey = [u8; 32];
    type PrivateKey = [u8; 32];
    const DROP_MESSAGE: &'static [u8] = &[0; MESSAGE_SIZE];

    /// The length of the encoded message is fixed to [`MESSAGE_SIZE`] bytes.
    /// The [`MAX_LAYERS`] number of [`NodeId`]s are concatenated in front of the payload.
    /// The payload is zero-padded to the end.
    ///
    fn build_message(payload: &[u8], public_keys: &[Self::PublicKey]) -> Result<Vec<u8>, Error> {
        if public_keys.is_empty() || public_keys.len() > MAX_LAYERS {
            return Err(Error::InvalidNumberOfLayers);
        }
        if payload.len() > PADDED_PAYLOAD_SIZE - PAYLOAD_PADDING_SEPARATOR_SIZE {
            return Err(Error::PayloadTooLarge);
        }

        let mut message: Vec<u8> = Vec::with_capacity(MESSAGE_SIZE);

        public_keys.iter().for_each(|public_key| {
            message.extend(public_key);
        });
        // If there is any remaining layers, fill them with zeros.
        (0..MAX_LAYERS - public_keys.len()).for_each(|_| message.extend(&DUMMY_NODE_ID));

        // Append payload with padding
        message.extend(payload);
        message.push(PAYLOAD_PADDING_SEPARATOR);
        message.extend(
            std::iter::repeat(0)
                .take(PADDED_PAYLOAD_SIZE - payload.len() - PAYLOAD_PADDING_SEPARATOR_SIZE),
        );
        Ok(message)
    }

    fn unwrap_message(
        message: &[u8],
        private_key: &Self::PrivateKey,
    ) -> Result<(Vec<u8>, bool), Error> {
        let public_key_size = std::mem::size_of::<Self::PublicKey>();

        if message.len() != MESSAGE_SIZE {
            return Err(Error::InvalidMixMessage);
        }

        let public_key =
            x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(*private_key))
                .to_bytes();
        if message[0..public_key_size] != public_key {
            return Err(Error::MsgUnwrapNotAllowed);
        }

        // If this is the last layer
        if message[public_key_size..public_key_size * 2] == DUMMY_NODE_ID {
            let padded_payload = &message[public_key_size * MAX_LAYERS..];
            // remove the payload padding
            match padded_payload
                .iter()
                .rposition(|&x| x == PAYLOAD_PADDING_SEPARATOR)
            {
                Some(pos) => {
                    println!("pos: {}", pos);
                    return Ok((padded_payload[0..pos].to_vec(), true));
                }
                None => return Err(Error::InvalidMixMessage),
            }
        }

        let mut new_message: Vec<u8> = Vec::with_capacity(MESSAGE_SIZE);
        new_message.extend(&message[public_key_size..public_key_size * MAX_LAYERS]);
        new_message.extend(&DUMMY_NODE_ID);
        new_message.extend(&message[public_key_size * MAX_LAYERS..]); // padded payload
        Ok((new_message, false))
    }
}

#[cfg(test)]
mod tests {
    use crate::NODE_ID_SIZE;

    use super::*;

    #[test]
    fn message() {
        let node_ids = [[1; NODE_ID_SIZE], [2; NODE_ID_SIZE], [3; NODE_ID_SIZE]];
        let payload = [7; 10];
        let message = new_message(&payload, &node_ids).unwrap();
        assert_eq!(message.len(), MESSAGE_SIZE);

        let (message, is_fully_unwrapped) = unwrap_message(&message, &node_ids[0]).unwrap();
        assert!(!is_fully_unwrapped);
        assert_eq!(message.len(), MESSAGE_SIZE);

        let (message, is_fully_unwrapped) = unwrap_message(&message, &node_ids[1]).unwrap();
        assert!(!is_fully_unwrapped);
        assert_eq!(message.len(), MESSAGE_SIZE);

        let (unwrapped_payload, is_fully_unwrapped) =
            super::unwrap_message(&message, &node_ids[2]).unwrap();
        assert!(is_fully_unwrapped);
        assert_eq!(unwrapped_payload, payload);
    }
}
