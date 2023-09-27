use mixnet_protocol::ProtocolError;
use nym_sphinx::addressing::nodes::NymNodeRoutingAddressError;

#[derive(thiserror::Error, Debug)]
pub enum MixnetClientError {
    #[error("mixnet node connect error")]
    MixnetNodeConnectError,
    #[error("mixnode stream has been closed")]
    MixnetNodeStreamClosed,
    #[error("unexpected stream body received")]
    UnexpectedStreamBody,
    #[error("invalid payload")]
    InvalidPayload,
    #[error("invalid fragment")]
    InvalidFragment,
    #[error("invalid routing address: {0}")]
    InvalidRoutingAddress(#[from] NymNodeRoutingAddressError),
    #[error("{0}")]
    Protocol(#[from] ProtocolError),
    #[error("{0}")]
    Message(#[from] nym_sphinx::message::NymMessageError),
}

pub type Result<T> = core::result::Result<T, MixnetClientError>;
