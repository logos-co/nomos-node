/// Mixnet Errors
#[derive(thiserror::Error, Debug)]
pub enum MixnetError {
    /// Invalid packet flag
    #[error("invalid packet flag")]
    InvalidPacketFlag,
    /// Node address error
    #[error("node address error: {0}")]
    NodeAddressError(#[from] nym_sphinx_addressing::nodes::NymNodeRoutingAddressError),
    /// Sphinx packet error
    #[error("sphinx packet error: {0}")]
    SphinxPacketError(#[from] sphinx_packet::Error),
    /// Exponential distribution error
    #[error("exponential distribution error: {0}")]
    ExponentialError(#[from] rand_distr::ExpError),
}
