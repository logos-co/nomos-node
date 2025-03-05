use sphinx_packet::header::routing::RoutingFlag;

use crate::sphinx::layered_cipher::CipherError;

#[derive(thiserror::Error, Debug)]
pub enum SphinxError {
    #[error("Sphinx packet error: {0}")]
    SphinxPacketError(#[from] sphinx_packet::Error),
    #[error("Invalid packet bytes")]
    InvalidPacketBytes,
    #[error("Invalid routing flag: {0}")]
    InvalidRoutingFlag(RoutingFlag),
    #[error("Invalid routing length: {0} bytes")]
    InvalidEncryptedRoutingInfoLength(usize),
    #[error("ConsistentLengthLayeredEncryptionError: {0}")]
    ConsistentLengthLayeredEncryptionError(#[from] CipherError),
}
