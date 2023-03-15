// std
use std::error::Error;
use std::hash::Hash;
// crates
use futures::StreamExt;
use serde::de::DeserializeOwned;
use serde::Serialize;
// internal
use super::*;
use crate::network::messages::{ProposalChunkMsg, VoteMsg};
use crate::network::NetworkAdapter;
use crate::overlay::committees::Committee;
use nomos_core::wire::deserializer;

const FLAT_COMMITTEE: Committee = Committee::root();

/// A flat overlay, everyone is in the same committee.
/// As far as the API is concerned, this should be equivalent to any other
/// overlay and far simpler to implement.
/// For this reason, this might act as a 'reference' overlay for testing.
pub struct Flat<TxId> {
    // TODO: this should be a const param, but we can't do that yet
    node_id: NodeId,
    view_n: u64,
    _marker: std::marker::PhantomData<TxId>,
}

impl<TxId: Eq + Hash> Flat<TxId> {
    pub fn new(view_n: u64, node_id: NodeId) -> Self {
        Self {
            node_id,
            view_n,
            _marker: Default::default(),
        }
    }

    fn approve(&self, _block: &Block<TxId>) -> Approval {
        // we still need to define how votes look like
        Approval
    }
}

#[async_trait::async_trait]
impl<Network, Fountain, VoteTally, TxId> Overlay<Network, Fountain, VoteTally> for Flat<TxId>
where
    TxId: serde::de::DeserializeOwned + Clone + Eq + Hash + Send + Sync + 'static,
    Network: NetworkAdapter + Sync,
    Fountain: FountainCode + Sync,
    VoteTally: Tally + Sync,
    VoteTally::Vote: Serialize + DeserializeOwned + Send,
{
    type TxId = TxId;

    fn new(view: &View, node: NodeId) -> Self {
        Flat::new(view.view_n, node)
    }

    async fn reconstruct_proposal_block(
        &self,
        view: &View,
        adapter: &Network,
        fountain: &Fountain,
    ) -> Result<Block<Self::TxId>, FountainError> {
        assert_eq!(view.view_n, self.view_n, "view_n mismatch");
        let message_stream = adapter.proposal_chunks_stream(FLAT_COMMITTEE, view).await;
        fountain.decode(message_stream).await.and_then(|b| {
            deserializer(&b)
                .deserialize::<Block<Self::TxId>>()
                .map_err(|e| FountainError::from(e.to_string().as_str()))
        })
    }

    async fn broadcast_block(
        &self,
        view: &View,
        block: Block<Self::TxId>,
        adapter: &Network,
        fountain: &Fountain,
    ) {
        assert_eq!(view.view_n, self.view_n, "view_n mismatch");
        let block_bytes = block.as_bytes();
        let encoded_stream = fountain.encode(&block_bytes);
        encoded_stream
            .for_each_concurrent(None, |chunk| async move {
                let message = ProposalChunkMsg { chunk };
                adapter
                    .broadcast_block_chunk(FLAT_COMMITTEE, view, message)
                    .await;
            })
            .await;
    }

    async fn approve_and_forward(
        &self,
        view: &View,
        block: &Block<Self::TxId>,
        adapter: &Network,
        _tally: &VoteTally,
        _next_view: &View,
    ) -> Result<(), Box<dyn Error>> {
        assert_eq!(view.view_n, self.view_n, "view_n mismatch");
        // in the flat overlay, there's no need to wait for anyone before approving the block
        let approval = self.approve(block);
        adapter
            .forward_approval(
                FLAT_COMMITTEE,
                view,
                VoteMsg {
                    vote: approval,
                    source: self.node_id,
                },
            )
            .await;
        Ok(())
    }

    async fn build_qc(
        &self,
        view: &View,
        adapter: &Network,
        tally: &VoteTally,
    ) -> VoteTally::Outcome {
        assert_eq!(view.view_n, self.view_n, "view_n mismatch");
        eprintln!("view {:?}", view);
        // for now, let's pretend that consensus is reached as soon as the
        // block is approved by a share of the nodes
        let stream = Box::into_pin(adapter.votes_stream(FLAT_COMMITTEE, view).await);
        eprintln!("asdasdasd");
        // Shadow the original binding so that it can't be directly accessed
        // ever again.
        if let Ok(qc) = tally.tally(view.view_n, stream).await {
            println!("tally");
            qc
        } else {
            unimplemented!("consensus not reached")
        }
    }
}
