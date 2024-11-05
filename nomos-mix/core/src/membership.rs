use multiaddr::Multiaddr;
use nomos_mix_message::MixMessage;
use rand::{seq::SliceRandom, Rng};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct Membership<M>
where
    M: MixMessage,
    M::PublicKey: Clone,
{
    remote_nodes: Vec<Node<M::PublicKey>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node<K>
where
    K: Clone,
{
    pub address: Multiaddr,
    pub public_key: K,
}

impl<M> Membership<M>
where
    M: MixMessage,
    M::PublicKey: Clone + Serialize + DeserializeOwned + PartialEq,
{
    pub fn new(mut nodes: Vec<Node<M::PublicKey>>, local_public_key: M::PublicKey) -> Self {
        nodes.retain(|node| node.public_key != local_public_key);
        Self {
            remote_nodes: nodes,
        }
    }

    pub fn choose_remote_nodes<R: Rng>(
        &self,
        rng: &mut R,
        amount: usize,
    ) -> Vec<&Node<M::PublicKey>> {
        self.remote_nodes.choose_multiple(rng, amount).collect()
    }
}
