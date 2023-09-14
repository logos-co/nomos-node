use std::net::SocketAddr;

use nym_sphinx::addressing::nodes::{NymNodeRoutingAddress, NymNodeRoutingAddressError};
use rand::{seq::IteratorRandom, Rng};
use serde::{Deserialize, Serialize};
use sphinx_packet::{crypto::PUBLIC_KEY_SIZE, route};

pub type MixnetNodeId = [u8; PUBLIC_KEY_SIZE];

pub type Result<T> = core::result::Result<T, NymNodeRoutingAddressError>;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct MixnetTopology {
    pub layers: Vec<Layer>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Layer {
    pub nodes: Vec<Node>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Node {
    pub address: SocketAddr,
    #[serde(with = "hex_serde")]
    pub public_key: [u8; PUBLIC_KEY_SIZE],
}

mod hex_serde {
    use super::PUBLIC_KEY_SIZE;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(
        pk: &[u8; PUBLIC_KEY_SIZE],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            hex::encode(pk).serialize(serializer)
        } else {
            serializer.serialize_bytes(pk)
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[u8; PUBLIC_KEY_SIZE], D::Error> {
        if deserializer.is_human_readable() {
            let hex_str = String::deserialize(deserializer)?;
            hex::decode(hex_str)
                .map_err(serde::de::Error::custom)
                .and_then(|v| v.as_slice().try_into().map_err(serde::de::Error::custom))
        } else {
            <[u8; PUBLIC_KEY_SIZE]>::deserialize(deserializer)
        }
    }
}

impl MixnetTopology {
    pub fn random_route<R: Rng>(&self, rng: &mut R) -> Result<Vec<route::Node>> {
        let num_hops = self.layers.len();

        let route: Vec<route::Node> = self
            .layers
            .iter()
            .take(num_hops)
            .map(|layer| {
                layer
                    .random_node(rng)
                    .expect("layer is not empty")
                    .clone()
                    .try_into()
                    .unwrap()
            })
            .collect();

        Ok(route)
    }

    // Choose a destination mixnet node randomly from the last layer.
    pub fn random_destination<R: Rng>(
        &self,
        rng: &mut R,
    ) -> core::result::Result<route::Node, NymNodeRoutingAddressError> {
        Ok(self
            .layers
            .last()
            .expect("topology is not empty")
            .random_node(rng)
            .expect("layer is not empty")
            .clone()
            .try_into()
            .unwrap())
    }
}

impl Layer {
    pub fn random_node<R: Rng>(&self, rng: &mut R) -> Option<&Node> {
        self.nodes.iter().choose(rng)
    }
}

impl TryInto<route::Node> for Node {
    type Error = NymNodeRoutingAddressError;

    fn try_into(self) -> core::result::Result<route::Node, Self::Error> {
        Ok(route::Node {
            address: NymNodeRoutingAddress::from(self.address).try_into()?,
            pub_key: self.public_key.into(),
        })
    }
}
