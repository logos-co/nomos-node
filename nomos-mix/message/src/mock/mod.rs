use crate::{Error, MixMessage};
// TODO: Remove all the mock below once the actual implementation is integrated to the system.
//
/// A mock implementation of the Sphinx encoding.

const PRIVATE_KEY_SIZE: usize = 32;
const PUBLIC_KEY_SIZE: usize = 32;

const PADDED_PAYLOAD_SIZE: usize = 2048;
const PAYLOAD_PADDING_SEPARATOR: u8 = 0x01;
const PAYLOAD_PADDING_SEPARATOR_SIZE: usize = 1;
const MAX_LAYERS: usize = 5;
pub const MESSAGE_SIZE: usize = PUBLIC_KEY_SIZE * MAX_LAYERS + PADDED_PAYLOAD_SIZE;

#[derive(Clone, Debug)]
pub struct MockMixMessage;

impl MixMessage for MockMixMessage {
    type PublicKey = [u8; PUBLIC_KEY_SIZE];
    type PrivateKey = [u8; PRIVATE_KEY_SIZE];
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
        (0..MAX_LAYERS - public_keys.len()).for_each(|_| message.extend(&[0; PUBLIC_KEY_SIZE]));

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

        let public_key =
            x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(*private_key))
                .to_bytes();
        if message[0..PUBLIC_KEY_SIZE] != public_key {
            return Err(Error::MsgUnwrapNotAllowed);
        }

        // If this is the last layer
        if message[PUBLIC_KEY_SIZE..PUBLIC_KEY_SIZE * 2] == [0; PUBLIC_KEY_SIZE] {
            let padded_payload = &message[PUBLIC_KEY_SIZE * MAX_LAYERS..];
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
        new_message.extend(&message[PUBLIC_KEY_SIZE..PUBLIC_KEY_SIZE * MAX_LAYERS]);
        new_message.extend(&[0; PUBLIC_KEY_SIZE]);
        new_message.extend(&message[PUBLIC_KEY_SIZE * MAX_LAYERS..]); // padded payload
        Ok((new_message, false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message() {
        let private_keys = [
            x25519_dalek::StaticSecret::random(),
            x25519_dalek::StaticSecret::random(),
            x25519_dalek::StaticSecret::random(),
        ];
        let public_keys = private_keys
            .iter()
            .map(|k| x25519_dalek::PublicKey::from(k).to_bytes())
            .collect::<Vec<_>>();
        let payload = [7; 10];
        let message = MockMixMessage::build_message(&payload, &public_keys).unwrap();
        assert_eq!(message.len(), MESSAGE_SIZE);

        let (message, is_fully_unwrapped) =
            MockMixMessage::unwrap_message(&message, &private_keys[0].to_bytes()).unwrap();
        assert!(!is_fully_unwrapped);
        assert_eq!(message.len(), MESSAGE_SIZE);

        let (message, is_fully_unwrapped) =
            MockMixMessage::unwrap_message(&message, &private_keys[1].to_bytes()).unwrap();
        assert!(!is_fully_unwrapped);
        assert_eq!(message.len(), MESSAGE_SIZE);

        let (unwrapped_payload, is_fully_unwrapped) =
            MockMixMessage::unwrap_message(&message, &private_keys[2].to_bytes()).unwrap();
        assert!(is_fully_unwrapped);
        assert_eq!(unwrapped_payload, payload);
    }
}
