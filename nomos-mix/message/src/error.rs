#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Invalid mix message format")]
    InvalidMixMessage,
    #[error("Integrity MAC verification failed")]
    IntegrityMacVerificationFailed,
    #[error("Payload is too large")]
    PayloadTooLarge,
    #[error("Too many recipients")]
    TooManyRecipients,
    #[error("Sphinx packet error: {0}")]
    SphinxPacketError(#[from] sphinx_packet::Error),
    #[error("Invalid routing flag: {0}")]
    InvalidRoutingFlag(u8),
    #[error("Invalid routing length: {0}")]
    InvalidRoutingInfoLength(usize),
    #[error("Unwrapping a message is not allowed to this node")]
    /// e.g. the message cannot be unwrapped using the private key provided
    MsgUnwrapNotAllowed,
}
