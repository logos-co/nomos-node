use std::{collections::HashMap, error::Error, net::SocketAddr};

use nym_sphinx::addressing::nodes::NymNodeRoutingAddress;
use rand::{seq::IteratorRandom, Rng};
use serde::{Deserialize, Serialize};
use sphinx_packet::{crypto::PUBLIC_KEY_SIZE, route};

pub type LayerId = u8;
pub type MixnodeId = [u8; PUBLIC_KEY_SIZE];

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Topology {
    pub layers: Vec<Layer>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Layer {
    pub id: LayerId,
    pub nodes: HashMap<MixnodeId, Mixnode>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Mixnode {
    pub address: SocketAddr,
    pub public_key: [u8; PUBLIC_KEY_SIZE],
}

impl Topology {
    pub fn layer_id(&self, node_id: MixnodeId) -> Option<LayerId> {
        for layer in self.layers.iter() {
            if layer.nodes.contains_key(&node_id) {
                return Some(layer.id);
            }
        }
        None
    }

    pub fn random_route<R: Rng>(
        &self,
        rng: &mut R,
        num_hops: usize,
    ) -> Result<Vec<route::Node>, Box<dyn Error>> {
        if self.layers.len() < num_hops {
            todo!("return error");
        }

        let mut route: Vec<route::Node> = Vec::new();

        for layer_id in 0..num_hops {
            let layer = self.layers.get(layer_id).unwrap();
            route.push(
                layer
                    .random_node(rng)
                    .expect("layer is not empty")
                    .clone()
                    .try_into()
                    .unwrap(),
            );
        }

        Ok(route)
    }
}

impl Layer {
    pub fn random_node<R: Rng>(&self, rng: &mut R) -> Option<&Mixnode> {
        self.nodes.values().choose(rng)
    }
}

impl TryInto<route::Node> for Mixnode {
    type Error = ();

    fn try_into(self) -> Result<route::Node, Self::Error> {
        Ok(route::Node {
            address: NymNodeRoutingAddress::from(self.address)
                .try_into()
                .unwrap(),
            pub_key: self.public_key.into(),
        })
    }
}
