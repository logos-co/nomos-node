use multiaddr::Multiaddr;
use rand::{seq::SliceRandom, Rng};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct Membership {
    remote_nodes: Vec<Node>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub address: Multiaddr,
    pub public_key: [u8; 32],
}

impl Membership {
    pub fn new(mut nodes: Vec<Node>, local_public_key: &[u8; 32]) -> Self {
        nodes.retain(|node| node.public_key != *local_public_key);
        Self {
            remote_nodes: nodes,
        }
    }

    pub fn choose_remote_nodes<R: Rng>(&self, rng: &mut R, amount: usize) -> Vec<&Node> {
        self.remote_nodes.choose_multiple(rng, amount).collect()
    }
}
