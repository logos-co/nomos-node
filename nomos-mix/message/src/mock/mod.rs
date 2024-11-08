use crate::{Error, MixMessage};
// TODO: Remove all the mock below once the actual implementation is integrated to the system.
//
/// A mock implementation of the Sphinx encoding.

const NODE_ID_SIZE: usize = 32;

const PADDED_PAYLOAD_SIZE: usize = 2048;
const PAYLOAD_PADDING_SEPARATOR: u8 = 0x01;
const PAYLOAD_PADDING_SEPARATOR_SIZE: usize = 1;
const MAX_LAYERS: usize = 5;
pub const MESSAGE_SIZE: usize = NODE_ID_SIZE * MAX_LAYERS + PADDED_PAYLOAD_SIZE;

#[derive(Clone, Debug)]
pub struct MockMixMessage;

impl MixMessage for MockMixMessage {
    type PublicKey = [u8; NODE_ID_SIZE];
    type PrivateKey = [u8; NODE_ID_SIZE];
    const DROP_MESSAGE: &'static [u8] = &[0; MESSAGE_SIZE];

    /// The length of the encoded message is fixed to [`MESSAGE_SIZE`] bytes.
    /// The [`MAX_LAYERS`] number of [`NodeId`]s are concatenated in front of the payload.
    /// The payload is zero-padded to the end.
    ///
    fn build_message(payload: &[u8], public_keys: &[Self::PublicKey]) -> Result<Vec<u8>, Error> {
        // In this mock, we don't encrypt anything. So, we use public key as just a node ID.
        let node_ids = public_keys;
        if node_ids.is_empty() || node_ids.len() > MAX_LAYERS {
            return Err(Error::InvalidNumberOfLayers);
        }
        if payload.len() > PADDED_PAYLOAD_SIZE - PAYLOAD_PADDING_SEPARATOR_SIZE {
            return Err(Error::PayloadTooLarge);
        }

        let mut message: Vec<u8> = Vec::with_capacity(MESSAGE_SIZE);

        node_ids.iter().for_each(|node_id| {
            message.extend(node_id);
        });
        // If there is any remaining layers, fill them with zeros.
        (0..MAX_LAYERS - node_ids.len()).for_each(|_| message.extend(&[0; NODE_ID_SIZE]));

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
        if message.len() != MESSAGE_SIZE {
            return Err(Error::InvalidMixMessage);
        }

        // In this mock, we don't decrypt anything. So, we use private key as just a node ID.
        let node_id = private_key;
        if &message[0..NODE_ID_SIZE] != node_id {
            return Err(Error::MsgUnwrapNotAllowed);
        }

        // If this is the last layer
        if message[NODE_ID_SIZE..NODE_ID_SIZE * 2] == [0; NODE_ID_SIZE] {
            let padded_payload = &message[NODE_ID_SIZE * MAX_LAYERS..];
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

        let mut new_message: Vec<u8> = Vec::with_capacity(MESSAGE_SIZE);
        new_message.extend(&message[NODE_ID_SIZE..NODE_ID_SIZE * MAX_LAYERS]);
        new_message.extend(&[0; NODE_ID_SIZE]);
        new_message.extend(&message[NODE_ID_SIZE * MAX_LAYERS..]); // padded payload
        Ok((new_message, false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message() {
        let node_ids = (0..3).map(|i| [i; NODE_ID_SIZE]).collect::<Vec<_>>();
        let payload = [7; 10];
        let message = MockMixMessage::build_message(&payload, &node_ids).unwrap();
        assert_eq!(message.len(), MESSAGE_SIZE);

        let (message, is_fully_unwrapped) =
            MockMixMessage::unwrap_message(&message, &node_ids[0]).unwrap();
        assert!(!is_fully_unwrapped);
        assert_eq!(message.len(), MESSAGE_SIZE);

        let (message, is_fully_unwrapped) =
            MockMixMessage::unwrap_message(&message, &node_ids[1]).unwrap();
        assert!(!is_fully_unwrapped);
        assert_eq!(message.len(), MESSAGE_SIZE);

        let (unwrapped_payload, is_fully_unwrapped) =
            MockMixMessage::unwrap_message(&message, &node_ids[2]).unwrap();
        assert!(is_fully_unwrapped);
        assert_eq!(unwrapped_payload, payload);
    }
}
