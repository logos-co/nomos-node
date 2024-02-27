use std::net::SocketAddr;

use nym_sphinx_addressing::nodes::NymNodeRoutingAddress;
use serde::{Deserialize, Serialize};

use crate::error::MixnetError;

/// Represents an address of mix node.
///
/// This just contains a single [`SocketAddr`], but has conversion functions
/// for various address types defined in the `sphinx-packet` crate.
#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq)]
pub struct NodeAddress(SocketAddr);

impl From<SocketAddr> for NodeAddress {
    fn from(address: SocketAddr) -> Self {
        Self(address)
    }
}

impl From<NodeAddress> for SocketAddr {
    fn from(address: NodeAddress) -> Self {
        address.0
    }
}

impl TryInto<sphinx_packet::route::NodeAddressBytes> for NodeAddress {
    type Error = MixnetError;

    fn try_into(self) -> Result<sphinx_packet::route::NodeAddressBytes, Self::Error> {
        Ok(NymNodeRoutingAddress::from(SocketAddr::from(self)).try_into()?)
    }
}

impl TryFrom<sphinx_packet::route::NodeAddressBytes> for NodeAddress {
    type Error = MixnetError;

    fn try_from(value: sphinx_packet::route::NodeAddressBytes) -> Result<Self, Self::Error> {
        Ok(Self::from(SocketAddr::from(
            NymNodeRoutingAddress::try_from(value)?,
        )))
    }
}

impl TryFrom<sphinx_packet::route::DestinationAddressBytes> for NodeAddress {
    type Error = MixnetError;

    fn try_from(value: sphinx_packet::route::DestinationAddressBytes) -> Result<Self, Self::Error> {
        Self::try_from(sphinx_packet::route::NodeAddressBytes::from_bytes(
            value.as_bytes(),
        ))
    }
}
