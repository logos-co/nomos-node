use std::{collections::HashSet, hash::Hash};

use multiaddr::Multiaddr;
use nomos_blend_message::BlendMessage;
use rand::{
    seq::{IteratorRandom, SliceRandom},
    Rng,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct Membership<NodeId, M>
where
    M: BlendMessage,
{
    remote_nodes: Vec<Node<NodeId, M::PublicKey>>,
    local_node: Node<NodeId, M::PublicKey>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node<NodeId, K> {
    pub id: NodeId,
    pub address: Multiaddr,
    pub public_key: K,
}

impl<NodeId, M> Membership<NodeId, M>
where
    M: BlendMessage,
    M::PublicKey: PartialEq,
    NodeId: Hash + Eq,
{
    pub fn new(nodes: Vec<Node<NodeId, M::PublicKey>>, local_public_key: M::PublicKey) -> Self {
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
    ) -> Vec<&Node<NodeId, M::PublicKey>> {
        self.remote_nodes.choose_multiple(rng, amount).collect()
    }

    pub fn filter_and_choose_remote_nodes<R: Rng>(
        &self,
        rng: &mut R,
        amount: usize,
        exclude_addrs: &HashSet<NodeId>,
    ) -> Vec<&Node<NodeId, M::PublicKey>> {
        self.remote_nodes
            .iter()
            .filter(|node| !exclude_addrs.contains(&node.id))
            .choose_multiple(rng, amount)
    }

    pub fn local_node(&self) -> &Node<NodeId, M::PublicKey> {
        &self.local_node
    }

    pub fn size(&self) -> usize {
        self.remote_nodes.len() + 1
    }
}
