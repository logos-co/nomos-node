#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Invalid blend message format")]
    InvalidBlendMessage,
    #[error("Payload is too large")]
    PayloadTooLarge,
    #[error("Invalid number of layers")]
    InvalidNumberOfLayers,
    #[error("Unwrapping a message is not allowed to this node")]
    /// e.g. the message cannot be unwrapped using the private key provided
    MsgUnwrapNotAllowed,
}
