// std
use std::fmt::Display;
use std::pin::Pin;
// crates
use futures::StreamExt;
use rand::{seq::SliceRandom, SeedableRng};
// internal
use super::*;
use crate::network::messages::ProposalChunkMsg;
use crate::network::NetworkAdapter;

/// View of the tree overlay centered around a specific member
pub struct Member<'view, const C: usize> {
    id: NodeId,
    committee: Committee,
    committees: Committees<'view, C>,
}

/// #Just a newtype index to be able to implement parent/children methods
#[derive(Copy, Clone)]
pub struct Committee(usize);

pub struct Committees<'view, const C: usize> {
    view: &'view View,
    nodes: Box<[NodeId]>,
}

impl<'view, const C: usize> Committees<'view, C> {
    pub fn new(view: &'view View) -> Self {
        let mut nodes = view.staking_keys.keys().cloned().collect::<Box<[NodeId]>>();
        let mut rng = rand_chacha::ChaCha20Rng::from_seed(view.seed);
        nodes.shuffle(&mut rng);
        Self { nodes, view }
    }

    pub fn into_member(self, id: NodeId) -> Option<Member<'view, C>> {
        let member_idx = self.nodes.iter().position(|m| m == &id)?;
        Some(Member {
            committee: Committee(member_idx / C),
            committees: self,
            id,
        })
    }

    fn get_committee_members(&self, committee: Committee) -> Option<&[NodeId]> {
        let leftb = committee.0 * C;
        let rightb = std::cmp::min(self.nodes.len(), leftb + C);

        if leftb < rightb {
            Some(&self.nodes[leftb..rightb])
        } else {
            None
        }
    }
}

impl Committee {
    pub fn id(&self) -> usize {
        self.0
    }
    /// Return the left and right children committee, if any
    pub fn children(&self) -> (Committee, Committee) {
        (
            // left child
            Committee(self.0 * 2 + 1),
            // right child
            Committee(self.0 + 2 + 2),
        )
    }

    /// Return the parent committee, if any
    pub fn parent(&self) -> Option<Committee> {
        if self.0 == 0 {
            None
        } else {
            Some(Committee((self.0 - 1) / 2))
        }
    }
}

impl<'view, const C: usize> Member<'view, C> {
    /// Return other members of this committee
    pub fn peers(&self) -> &[NodeId] {
        self.committees
            .get_committee_members(self.committee)
            .unwrap()
    }

    /// Return the participant in the parent committee this member should interact
    /// with
    pub fn parent_committee(&self) -> Option<Committee> {
        self.committee.parent()
    }

    // Return participants in the children committees this member should interact with
    pub fn children_committes(&self) -> (Option<Committee>, Option<Committee>) {
        let (left, right) = self.committee.children();
        (
            self.committees.get_committee_members(left).map(|_| left),
            self.committees.get_committee_members(right).map(|_| right),
        )
    }
}

#[async_trait::async_trait]
impl<'view, Network: NetworkAdapter + Sync, Fountain: FountainCode + Sync, const C: usize>
    Overlay<'view, Network, Fountain> for Member<'view, C>
{
    fn new(view: &'view View, node: NodeId) -> Self {
        let committees = Committees::new(view);
        committees.into_member(node).unwrap()
    }

    async fn reconstruct_proposal_block(
        &self,
        adapter: &Network,
        fountain: &Fountain,
    ) -> Result<Block, FountainError> {
        let committee = self.committee;
        let view = self.committees.view;
        let message_stream = adapter.proposal_chunks_stream(committee, view).await;
        fountain.decode(message_stream).await.map(Block::from_bytes)
    }

    async fn broadcast_block(&self, block: Block, adapter: &Network, fountain: &Fountain) {
        let (left_child, right_child) = self.children_committes();
        let view = self.committees.view;
        let block_bytes = block.as_bytes();
        let encoded_stream = fountain.encode(&block_bytes);
        encoded_stream
            .for_each_concurrent(None, |chunk| async move {
                let message = ProposalChunkMsg { chunk };
                futures::join!(
                    adapter.broadcast_block_chunk(left_child, view, message.clone()),
                    adapter.broadcast_block_chunk(right_child, view, message.clone()),
                );
            })
            .await;
    }

    async fn approve_and_forward(
        &self,
        _block: &Block,
        adapter: &Network,
    ) -> Result<(), Box<dyn Error>> {
        // roughly, we want to do something like this:
        // 1. wait for left and right children committees to approve
        // 2. approve the block
        // 3. forward the approval to the parent committee
        //
        // However this will likely change depending on the position
        // of the committee in the tree
        todo!()
    }

    async fn wait_for_consensus(&self, _approval: &Block, adapter: &Network) {
        // maybe the leader publishing the QC?
    }
}
