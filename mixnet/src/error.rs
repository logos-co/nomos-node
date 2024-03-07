/// Mixnet Errors
#[derive(thiserror::Error, Debug)]
pub enum MixnetError {
    /// Invalid packet flag
    #[error("invalid packet flag")]
    InvalidPacketFlag,
    /// Node address error
    #[error("node address error: {0}")]
    NodeAddressError(#[from] nym_sphinx_addressing::nodes::NymNodeRoutingAddressError),
}
