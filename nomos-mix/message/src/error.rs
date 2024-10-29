#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Invalid mix message format")]
    InvalidMixMessage,
    #[error("Payload is too large")]
    PayloadTooLarge,
    #[error("Sphinx packet error: {0}")]
    SphinxPacketError(#[from] sphinx_packet::Error),
    #[error("Invalid packet")]
    InvalidPacket,
    #[error("Invalid routing flag: {0}")]
    InvalidRoutingFlag(u8),
    #[error("Invalid routing length: {0}")]
    InvalidEncryptedRoutingInfoLength(usize),
    #[error("ConsistentLengthLayeredEncryptionError: {0}")]
    ConsistentLengthLayeredEncryptionError(#[from] crate::layered_cipher::Error),
    #[error("Unwrapping a message is not allowed to this node")]
    /// e.g. the message cannot be unwrapped using the private key provided
    MsgUnwrapNotAllowed,
}
