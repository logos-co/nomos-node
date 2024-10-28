#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Invalid mix message format")]
    InvalidMixMessage,
    #[error("Payload is too large")]
    PayloadTooLarge,
    #[error("Failed to encrypt/decrypt payload: {0}")]
    PayloadCryptoError(#[from] lioness::LionessError),
    #[error("Too many recipients")]
    TooManyRecipients,
    #[error("Unwrapping a message is not allowed to this node")]
    /// e.g. the message cannot be unwrapped using the private key provided
    MsgUnwrapNotAllowed,
}
