#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("Invalid blend message format")]
    InvalidBlendMessage,
    #[error("Payload is too large")]
    PayloadTooLarge,
    #[error("Invalid number of layers")]
    InvalidNumberOfLayers,
    #[error("Invalid public key")]
    InvalidPublicKey,
    #[error("Unwrapping a message is not allowed to this node")]
    /// e.g. the message cannot be unwrapped using the private key provided
    MsgUnwrapNotAllowed,
}
