#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Sphinx packet error: {0}")]
    SphinxPacketError(#[from] sphinx_packet::Error),
    #[error("Invalid packet bytes")]
    InvalidPacketBytes,
    #[error("Invalid routing flag: {0}")]
    InvalidRoutingFlag(u8),
    #[error("Invalid routing length: {0}")]
    InvalidEncryptedRoutingInfoLength(usize),
    #[error("ConsistentLengthLayeredEncryptionError: {0}")]
    ConsistentLengthLayeredEncryptionError(#[from] super::layered_cipher::Error),
}
