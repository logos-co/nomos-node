use nomos_utils::fisheryates::FisherYatesShuffle;
use rand::Rng;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sphinx_packet::{
    constants::IDENTIFIER_LENGTH,
    crypto::{PublicKey, PUBLIC_KEY_SIZE},
    route::{DestinationAddressBytes, SURBIdentifier},
};

use crate::{address::NodeAddress, error::MixnetError};

/// Defines Mixnet topology construction and route selection
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixnetTopology {
    mixnode_candidates: Vec<MixNodeInfo>,
    num_layers: usize,
    num_mixnodes_per_layer: usize,
}

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
        if mixnode_candidates.len() < num_layers * num_mixnodes_per_layer {
            return Err(MixnetError::InvalidTopologySize);
        }

        FisherYatesShuffle::shuffle(&mut mixnode_candidates, entropy);
        Ok(Self {
            mixnode_candidates,
            num_layers,
            num_mixnodes_per_layer,
        })
    }

    /// Selects a mix destination randomly from the last mix layer
    pub(crate) fn choose_destination(&self) -> sphinx_packet::route::Destination {
        let idx_in_layer = rand::thread_rng().gen_range(0..self.num_mixnodes_per_layer);
        let idx = self.num_mixnodes_per_layer * (self.num_layers - 1) + idx_in_layer;
        self.mixnode_candidates[idx].clone().into()
    }

    /// Selects a mix route randomly from all mix layers except the last layer
    /// and append a mix destination to the end of the mix route.
    ///
    /// That is, the caller can generate multiple routes with one mix destination.
    pub(crate) fn gen_route(&self) -> Vec<sphinx_packet::route::Node> {
        let mut route = Vec::with_capacity(self.num_layers);
        for layer in 0..self.num_layers - 1 {
            let idx_in_layer = rand::thread_rng().gen_range(0..self.num_mixnodes_per_layer);
            let idx = self.num_mixnodes_per_layer * layer + idx_in_layer;
            route.push(self.mixnode_candidates[idx].clone().into());
        }
        route
    }
}

/// Mix node information that is used for forwarding packets to the mix node
#[derive(Clone, Debug)]
pub struct MixNodeInfo(sphinx_packet::route::Node);

impl MixNodeInfo {
    /// Creates a [`MixNodeInfo`].
    pub fn new(
        address: NodeAddress,
        public_key: [u8; PUBLIC_KEY_SIZE],
    ) -> Result<Self, MixnetError> {
        Ok(Self(sphinx_packet::route::Node::new(
            address.try_into()?,
            PublicKey::from(public_key),
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

impl Serialize for MixNodeInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        SerializableMixNodeInfo::try_from(self)
            .unwrap()
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MixNodeInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::try_from(SerializableMixNodeInfo::deserialize(deserializer)?).unwrap())
    }
}

// Only for serializing/deserializing [`MixNodeInfo`] since [`sphinx_packet::route::Node`] is not serializable.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct SerializableMixNodeInfo {
    address: NodeAddress,
    public_key: [u8; PUBLIC_KEY_SIZE],
}

impl TryFrom<&MixNodeInfo> for SerializableMixNodeInfo {
    type Error = MixnetError;

    fn try_from(info: &MixNodeInfo) -> Result<Self, Self::Error> {
        Ok(Self {
            address: NodeAddress::try_from(info.0.address)?,
            public_key: *info.0.pub_key.as_bytes(),
        })
    }
}

impl TryFrom<SerializableMixNodeInfo> for MixNodeInfo {
    type Error = MixnetError;

    fn try_from(info: SerializableMixNodeInfo) -> Result<Self, Self::Error> {
        Self::new(info.address, info.public_key)
    }
}

#[cfg(test)]
pub mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use rand::RngCore;
    use sphinx_packet::crypto::{PrivateKey, PublicKey};

    use crate::error::MixnetError;

    use super::{MixNodeInfo, MixnetTopology};

    #[test]
    fn shuffle() {
        let candidates = gen_mixnodes(10);
        let topology = MixnetTopology::new(candidates.clone(), 3, 2, gen_entropy()).unwrap();

        assert_eq!(candidates.len(), topology.mixnode_candidates.len());
    }

    #[test]
    fn route_and_destination() {
        let topology = MixnetTopology::new(gen_mixnodes(10), 3, 2, gen_entropy()).unwrap();
        let _ = topology.choose_destination();
        assert_eq!(2, topology.gen_route().len()); // except a destination
    }

    #[test]
    fn invalid_topology_size() {
        // if # of candidates is smaller than the topology size
        assert!(matches!(
            MixnetTopology::new(gen_mixnodes(5), 3, 2, gen_entropy()).err(),
            Some(MixnetError::InvalidTopologySize),
        ));
    }

    pub fn gen_mixnodes(n: usize) -> Vec<MixNodeInfo> {
        (0..n)
            .map(|i| {
                MixNodeInfo::new(
                    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), i as u16).into(),
                    *PublicKey::from(&PrivateKey::new()).as_bytes(),
                )
                .unwrap()
            })
            .collect()
    }

    pub fn gen_entropy() -> [u8; 32] {
        let mut entropy = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut entropy);
        entropy
    }
}
