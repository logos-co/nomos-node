mod error;

pub use error::Error;

use sha2::{Digest, Sha256};

pub const MSG_SIZE: usize = 1024;
pub const NOISE: [u8; MSG_SIZE] = [0; MSG_SIZE];

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

pub fn is_noise(message: &[u8]) -> bool {
    message == NOISE
}
