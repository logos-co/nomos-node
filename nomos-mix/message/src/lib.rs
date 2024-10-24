mod error;

pub use error::Error;

use sha2::{Digest, Sha256};

pub const MSG_SIZE: usize = 2048;
pub const DROP_MESSAGE: [u8; MSG_SIZE] = [0; MSG_SIZE];

/// A mock implementation of the Sphinx encoding.
///
/// The length of the encoded message is fixed to [`MSG_SIZE`] bytes.
/// The first byte of the encoded message is the number of remaining layers to be unwrapped.
/// The remaining bytes are the payload that is zero-padded to the end.
pub fn new_message(payload: &[u8], num_layers: u8) -> Result<Vec<u8>, Error> {
    if payload.len() > MSG_SIZE - 1 {
        return Err(Error::PayloadTooLarge);
    }

    let mut message: Vec<u8> = Vec::with_capacity(MSG_SIZE);
    message.push(num_layers);
    message.extend(payload);
    message.extend(std::iter::repeat(0).take(MSG_SIZE - message.len()));
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
pub fn unwrap_message(message: &[u8]) -> Result<(Vec<u8>, bool), Error> {
    if message.is_empty() {
        return Err(Error::InvalidMixMessage);
    }

    match message[0] {
        0 => Err(Error::InvalidMixMessage),
        1 => Ok((message[1..].to_vec(), true)),
        n => {
            let mut unwrapped: Vec<u8> = Vec::with_capacity(message.len());
            unwrapped.push(n - 1);
            unwrapped.extend(&message[1..]);
            Ok((unwrapped, false))
        }
    }
}

/// Check if the message is a drop message.
pub fn is_drop_message(message: &[u8]) -> bool {
    message == DROP_MESSAGE
}
