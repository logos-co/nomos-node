mod error;
pub mod packet;

pub use error::Error;

use sha2::{Digest, Sha256};

// TODO: Remove all the mock below once the actual implementation is integrated to the system.
//
/// A mock implementation of the Sphinx encoding.

pub type NodeId = [u8; NODE_ID_SIZE];
const NODE_ID_SIZE: usize = 32;
const DUMMY_NODE_ID: NodeId = [0; NODE_ID_SIZE];

const MAX_PADDED_PAYLOAD_SIZE: usize = 2048;
const PAYLOAD_PADDING_SEPARATOR: u8 = 0x01;
const PAYLOAD_PADDING_SEPARATOR_SIZE: usize = 1;
const MAX_LAYERS: usize = 5;
const MESSAGE_SIZE: usize = NODE_ID_SIZE * MAX_LAYERS + MAX_PADDED_PAYLOAD_SIZE;
pub const DROP_MESSAGE: [u8; MESSAGE_SIZE] = [0; MESSAGE_SIZE];

/// The length of the encoded message is fixed to [`MESSAGE_SIZE`] bytes.
/// The [`MAX_LAYERS`] number of [`NodeId`]s are concatenated in front of the payload.
/// The payload is zero-padded to the end.
pub fn new_message(payload: &[u8], node_ids: &[NodeId]) -> Result<Vec<u8>, Error> {
    if node_ids.is_empty() || node_ids.len() > MAX_LAYERS {
        return Err(Error::InvalidNumberOfLayers);
    }
    if payload.len() > MAX_PADDED_PAYLOAD_SIZE - PAYLOAD_PADDING_SEPARATOR_SIZE {
        return Err(Error::PayloadTooLarge);
    }

    let mut message: Vec<u8> = Vec::with_capacity(MESSAGE_SIZE);

    node_ids.iter().for_each(|node_id| {
        message.extend(node_id);
    });
    // If there is any remaining layers, fill them with zeros.
    (0..MAX_LAYERS - node_ids.len()).for_each(|_| message.extend(&DUMMY_NODE_ID));

    // Append payload with padding
    message.extend(payload);
    message.push(PAYLOAD_PADDING_SEPARATOR);
    message.extend(
        std::iter::repeat(0)
            .take(MAX_PADDED_PAYLOAD_SIZE - payload.len() - PAYLOAD_PADDING_SEPARATOR_SIZE),
    );
    Ok(message)
}

/// SHA-256 hash of the message
pub fn message_id(message: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(message);
    hasher.finalize().to_vec()
}

/// Unwrap the message one layer.
///
/// This function returns the unwrapped message and a boolean indicating whether the message was fully unwrapped.
/// (False if the message still has layers to be unwrapped, true otherwise)
///
/// If the input message was already fully unwrapped, or if ititss format is invalid,
/// this function returns `[Error::InvalidMixMessage]`.
pub fn unwrap_message(message: &[u8], node_id: &NodeId) -> Result<(Vec<u8>, bool), Error> {
    if message.len() != MESSAGE_SIZE {
        return Err(Error::InvalidMixMessage);
    }
    if &message[0..NODE_ID_SIZE] != node_id {
        return Err(Error::MsgUnwrapNotAllowed);
    }

    // If this is the last layer
    if message[NODE_ID_SIZE..NODE_ID_SIZE * 2] == DUMMY_NODE_ID {
        let padded_payload = &message[NODE_ID_SIZE * MAX_LAYERS..];
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
    new_message.extend(&message[NODE_ID_SIZE..NODE_ID_SIZE * MAX_LAYERS]);
    new_message.extend(&DUMMY_NODE_ID);
    new_message.extend(&message[NODE_ID_SIZE * MAX_LAYERS..]); // padded payload
    Ok((new_message, false))
}

/// Check if the message is a drop message.
pub fn is_drop_message(message: &[u8]) -> bool {
    message == DROP_MESSAGE
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
