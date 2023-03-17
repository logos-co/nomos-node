// std
use std::hash::Hash;
// crates
use futures::StreamExt;
use nomos_core::wire::deserializer;
use rand::{seq::SliceRandom, SeedableRng};
// internal
use super::*;
use crate::network::messages::ProposalChunkMsg;
use crate::network::NetworkAdapter;

/// View of the tree overlay centered around a specific member
pub struct Member<const C: usize> {
    // id is not used now, but gonna probably used it for later checking later on
    #[allow(dead_code)]
    id: NodeId,
    committee: Committee,
    committees: Committees<C>,
    view_n: u64,
}

/// #Just a newtype index to be able to implement parent/children methods
#[derive(Copy, Clone)]
pub struct Committee(usize);

pub struct Committees<const C: usize> {
    nodes: Box<[NodeId]>,
}

impl<const C: usize> Committees<C> {
    pub fn new(view: &View) -> Self {
        let mut nodes = view.staking_keys.keys().cloned().collect::<Box<[NodeId]>>();
        let mut rng = rand_chacha::ChaCha20Rng::from_seed(view.seed);
        nodes.shuffle(&mut rng);
        Self { nodes }
    }

    pub fn into_member(self, id: NodeId, view: &View) -> Option<Member<C>> {
        let member_idx = self.nodes.iter().position(|m| m == &id)?;
        Some(Member {
            committee: Committee(member_idx / C),
            committees: self,
            id,
            view_n: view.view_n,
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
    pub const fn root() -> Self {
        Self(0)
    }

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

impl<const C: usize> Member<C> {
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
impl<Network, Fountain, VoteTally, TxId, const C: usize> Overlay<Network, Fountain, VoteTally, TxId>
    for Member<C>
where
    Network: NetworkAdapter + Sync,
    Fountain: FountainCode + Sync,
    VoteTally: Tally + Sync,
    VoteTally::Qc: serde::de::DeserializeOwned + Clone + Send + Sync,
    TxId: serde::de::DeserializeOwned + Clone + Hash + Eq + Send + Sync + 'static,
{
    // we still need view here to help us initialize
    fn new(view: &View, node: NodeId) -> Self {
        let committees = Committees::new(view);
        committees.into_member(node, view).unwrap()
    }

    async fn reconstruct_proposal_block(
        &self,
        view: &View,
        adapter: &Network,
        fountain: &Fountain,
    ) -> Result<Block<VoteTally::Qc, TxId>, FountainError> {
        assert_eq!(view.view_n, self.view_n, "view_n mismatch");
        let committee = self.committee;
        let message_stream = adapter.proposal_chunks_stream(committee, view).await;
        fountain.decode(message_stream).await.and_then(|b| {
            deserializer(&b)
                .deserialize::<Block<VoteTally::Qc, TxId>>()
                .map_err(|e| FountainError::from(e.to_string().as_str()))
        })
    }

    async fn broadcast_block(
        &self,
        view: &View,
        block: Block<VoteTally::Qc, TxId>,
        adapter: &Network,
        fountain: &Fountain,
    ) {
        assert_eq!(view.view_n, self.view_n, "view_n mismatch");
        let (left_child, right_child) = self.children_committes();
        let block_bytes = block.as_bytes();
        let encoded_stream = fountain.encode(&block_bytes);
        encoded_stream
            .for_each_concurrent(None, |chunk| async move {
                let message = ProposalChunkMsg { chunk };
                let r_child = right_child
                    .map(|right_child| {
                        adapter.broadcast_block_chunk(right_child, view, message.clone())
                    })
                    .into_iter();
                let l_child = left_child
                    .map(|left_child| {
                        adapter.broadcast_block_chunk(left_child, view, message.clone())
                    })
                    .into_iter();
                futures::future::join_all(r_child.chain(l_child)).await;
            })
            .await;
    }

    async fn approve_and_forward(
        &self,
        view: &View,
        _block: &Block<VoteTally::Qc, TxId>,
        _adapter: &Network,
        _tally: &VoteTally,
        _next_view: &View,
    ) -> Result<(), Box<dyn Error>> {
        assert_eq!(view.view_n, self.view_n, "view_n mismatch");
        // roughly, we want to do something like this:
        // 1. wait for left and right children committees to approve
        // 2. approve the block
        // 3. forward the approval to the parent committee
        //
        // However this will likely change depending on the position
        // of the committee in the tree
        todo!()
    }

    async fn build_qc(&self, view: &View, _adapter: &Network, _tally: &VoteTally) -> VoteTally::Qc {
        assert_eq!(view.view_n, self.view_n, "view_n mismatch");
        // maybe the leader publishing the QC?
        todo!()
    }
}
