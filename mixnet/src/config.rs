use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use nym_sphinx_addressing::nodes::NymNodeRoutingAddress;
use serde::{Deserialize, Serialize};
use sphinx_packet::{
    crypto::{PrivateKey, PRIVATE_KEY_SIZE, PUBLIC_KEY_SIZE},
    route::{self},
};

#[derive(Clone)]
pub struct Config {
    pub listen_address: SocketAddr,
    // An external address known to be (likely) reachable for other nodes
    pub external_address: SocketAddr,
    pub private_key: [u8; PRIVATE_KEY_SIZE],
    // TODO: find better ways to handle topology
    pub topology: Topology,
    pub num_hops: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 7777),
            external_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 7777),
            private_key: PrivateKey::new().to_bytes(),
            topology: HashMap::new(),
            num_hops: 3,
        }
    }
}

pub type Topology = HashMap<[u8; PUBLIC_KEY_SIZE], MixNode>;

#[derive(Clone, Serialize, Deserialize)]
pub struct MixNode {
    pub public_key: [u8; PUBLIC_KEY_SIZE],
    pub addr: SocketAddr,
}

impl TryInto<route::Node> for MixNode {
    type Error = ();

    fn try_into(self) -> Result<route::Node, Self::Error> {
        let addr: NymNodeRoutingAddress = self.addr.into();
        Ok(route::Node {
            address: addr.try_into().unwrap(),
            pub_key: self.public_key.into(),
        })
    }
}

impl MixNode {
    pub fn as_bytes(&self) -> Box<[u8]> {
        wire::serialize(self).unwrap().into_boxed_slice()
    }
    pub fn from_bytes(data: &[u8]) -> Self {
        wire::deserialize(data).unwrap()
    }
}
