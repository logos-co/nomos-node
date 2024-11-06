mod error;
pub mod mock;
pub mod packet;

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
