use std::{collections::HashMap, error::Error, net::SocketAddr};

use nym_sphinx::addressing::nodes::{NymNodeRoutingAddress, NymNodeRoutingAddressError};
use rand::{seq::IteratorRandom, Rng};
use serde::{Deserialize, Serialize};
use sphinx_packet::{crypto::PUBLIC_KEY_SIZE, route};

pub type MixnetNodeId = [u8; PUBLIC_KEY_SIZE];

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct MixnetTopology {
    pub layers: Vec<Layer>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Layer {
    pub nodes: HashMap<MixnetNodeId, Node>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Node {
    pub address: SocketAddr,
    pub public_key: [u8; PUBLIC_KEY_SIZE],
}

impl MixnetTopology {
    pub fn random_route<R: Rng>(
        &self,
        rng: &mut R,
        num_hops: usize,
    ) -> Result<Vec<route::Node>, Box<dyn Error>> {
        if self.layers.len() < num_hops {
            todo!("return error");
        }

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
}

impl Layer {
    pub fn random_node<R: Rng>(&self, rng: &mut R) -> Option<&Node> {
        self.nodes.values().choose(rng)
    }
}

impl TryInto<route::Node> for Node {
    type Error = NymNodeRoutingAddressError;

    fn try_into(self) -> Result<route::Node, Self::Error> {
        Ok(route::Node {
            address: NymNodeRoutingAddress::from(self.address).try_into()?,
            pub_key: self.public_key.into(),
        })
    }
}
