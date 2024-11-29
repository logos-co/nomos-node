use multiaddr::Multiaddr;
use nomos_mix_message::MixMessage;
use rand::{seq::SliceRandom, Rng};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct Membership<M>
where
    M: MixMessage,
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
    M: MixMessage,
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

    pub fn local_node(&self) -> &Node<M::PublicKey> {
        &self.local_node
    }

    pub fn size(&self) -> usize {
        self.remote_nodes.len() + 1
    }
}
