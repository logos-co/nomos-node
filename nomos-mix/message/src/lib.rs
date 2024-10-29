mod error;
mod layered_cipher;
pub mod mock;
pub mod packet;
mod routing;

pub use error::Error;

pub trait MixMessage {
    type PublicKey;
    type PrivateKey;
    const DROP_MESSAGE: &'static [u8];

    fn build_message(payload: &[u8], public_keys: &[Self::PublicKey]) -> Result<Vec<u8>, Error>;
    /// Unwrap the message one layer.
    ///
    /// This function returns the unwrapped message and a boolean indicating whether the message was fully unwrapped.
    /// (False if the message still has layers to be unwrapped, true otherwise)
    ///
    /// If the input message was already fully unwrapped, or if its format is invalid,
    /// this function returns `[Error::InvalidMixMessage]`.
    fn unwrap_message(
        message: &[u8],
        private_key: &Self::PrivateKey,
    ) -> Result<(Vec<u8>, bool), Error>;
    fn is_drop_message(message: &[u8]) -> bool {
        message == Self::DROP_MESSAGE
    }
}

pub(crate) fn concat_bytes(bytes_list: &[&[u8]]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(bytes_list.iter().map(|bytes| bytes.len()).sum());
    bytes_list
        .iter()
        .for_each(|bytes| buf.extend_from_slice(bytes));
    buf
}

pub(crate) fn parse_bytes<'a>(data: &'a [u8], sizes: &[usize]) -> Result<Vec<&'a [u8]>, String> {
    let mut i = 0;
    sizes
        .iter()
        .map(|&size| {
            if i + size > data.len() {
                return Err("The sum of sizes exceeds the length of the input slice".to_string());
            }
            let slice = &data[i..i + size];
            i += size;
            Ok(slice)
        })
        .collect()
}
