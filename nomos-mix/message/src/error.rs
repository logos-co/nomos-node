#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Invalid mix message format")]
    InvalidMixMessage,
    #[error("Payload is too large")]
    PayloadTooLarge,
    #[error("Invalid number of layers")]
    InvalidNumberOfLayers,
    #[error("Sphinx packet error: {0}")]
    SphinxPacketError(#[from] sphinx_packet::Error),
    #[error("Unwrapping a message is not allowed to this node")]
    /// e.g. the message cannot be unwrapped using the private key provided
    MsgUnwrapNotAllowed,
}
