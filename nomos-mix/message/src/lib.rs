pub mod mock;
pub mod sphinx;

pub trait MixMessage {
    type PublicKey;
    type PrivateKey;
    type Settings;
    type Error;

    fn new(settings: Self::Settings) -> Self;

    fn build_message(
        &self,
        payload: &[u8],
        public_keys: &[Self::PublicKey],
    ) -> Result<Vec<u8>, Self::Error>;
    /// Unwrap the message one layer.
    ///
    /// This function returns the unwrapped message and a boolean indicating whether the message was fully unwrapped.
    /// (False if the message still has layers to be unwrapped, true otherwise)
    ///
    /// If the input message was already fully unwrapped, or if its format is invalid,
    /// this function returns `[Error::InvalidMixMessage]`.
    fn unwrap_message(
        &self,
        message: &[u8],
        private_key: &Self::PrivateKey,
    ) -> Result<(Vec<u8>, bool), Self::Error>;

    fn drop_message(&self) -> &[u8];

    fn is_drop_message(&self, message: &[u8]) -> bool {
        message == self.drop_message()
    }
}
