use std::collections::HashSet;

use multiaddr::Multiaddr;
use nomos_blend_message::BlendMessage;
use rand::{
    seq::{IteratorRandom, SliceRandom},
    Rng,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct Membership<M>
where
    M: BlendMessage,
{
    remote_nodes: Vec<Node<M::PublicKey>>,
    local_node: Node<M::PublicKey>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node<K> {
    pub address: Multiaddr,
    pub public_key: K,
}

impl<M> Membership<M>
where
    M: BlendMessage,
    M::PublicKey: PartialEq,
{
    pub fn new(nodes: Vec<Node<M::PublicKey>>, local_public_key: M::PublicKey) -> Self {
        let mut remote_nodes = Vec::with_capacity(nodes.len() - 1);
        let mut local_node = None;
        nodes.into_iter().for_each(|node| {
            if node.public_key == local_public_key {
                local_node = Some(node);
            } else {
                remote_nodes.push(node);
            }
        });

        Self {
            remote_nodes,
            local_node: local_node.expect("Local node not found"),
        }
    }

    pub fn choose_remote_nodes<R: Rng>(
        &self,
        rng: &mut R,
        amount: usize,
    ) -> Vec<&Node<M::PublicKey>> {
        self.remote_nodes.choose_multiple(rng, amount).collect()
    }

    pub fn filter_and_choose_remote_nodes<R: Rng>(
        &self,
        rng: &mut R,
        amount: usize,
        exclude_addrs: &HashSet<Multiaddr>,
    ) -> Vec<&Node<M::PublicKey>> {
        self.remote_nodes
            .iter()
            .filter(|node| !exclude_addrs.contains(&node.address))
            .choose_multiple(rng, amount)
    }

    pub fn local_node(&self) -> &Node<M::PublicKey> {
        &self.local_node
    }

    pub fn size(&self) -> usize {
        self.remote_nodes.len() + 1
    }
}
