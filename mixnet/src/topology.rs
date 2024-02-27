use serde::{Deserialize, Serialize};
use sphinx_packet::{
    constants::IDENTIFIER_LENGTH,
    route::{DestinationAddressBytes, SURBIdentifier},
};

use crate::{address::NodeAddress, crypto::PublicKey, error::MixnetError};

/// Defines Mixnet topology construction and route selection
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixnetTopology {}

impl MixnetTopology {
    /// Generates [MixnetTopology] with random shuffling/sampling using a given entropy.
    ///
    /// # Errors
    ///
    /// This function will return an error if parameters are invalid.
    pub fn new(
        mut mixnode_candidates: Vec<MixNodeInfo>,
        num_layers: usize,
        num_mixnodes_per_layer: usize,
        entropy: [u8; 32],
    ) -> Result<Self, MixnetError> {
        todo!()
    }

    /// Selects a mix destination randomly from the last mix layer
    pub(crate) fn choose_destination(&self) -> sphinx_packet::route::Destination {
        todo!()
    }

    /// Selects a mix route randomly from all mix layers except the last layer
    /// and append a mix destination to the end of the mix route.
    ///
    /// That is, the caller can generate multiple routes with one mix destination.
    pub(crate) fn gen_route(&self) -> Vec<sphinx_packet::route::Node> {
        todo!()
    }
}

/// Mix node information that is used for forwarding packets to the mix node
#[derive(Clone, Debug)]
pub struct MixNodeInfo(sphinx_packet::route::Node);

impl MixNodeInfo {
    /// Creates a [`MixNodeInfo`].
    pub fn new(address: NodeAddress, public_key: PublicKey) -> Result<Self, MixnetError> {
        Ok(Self(sphinx_packet::route::Node::new(
            address.try_into()?,
            *public_key,
        )))
    }
}

impl From<MixNodeInfo> for sphinx_packet::route::Node {
    fn from(info: MixNodeInfo) -> Self {
        info.0
    }
}

const DUMMY_SURB_IDENTIFIER: SURBIdentifier = [0u8; IDENTIFIER_LENGTH];

impl From<MixNodeInfo> for sphinx_packet::route::Destination {
    fn from(info: MixNodeInfo) -> Self {
        sphinx_packet::route::Destination::new(
            DestinationAddressBytes::from_bytes(info.0.address.as_bytes()),
            DUMMY_SURB_IDENTIFIER,
        )
    }
}
