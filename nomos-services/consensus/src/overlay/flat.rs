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
pub struct Flat {
    // TODO: this should be a const param, but we can't do that yet
    node_id: NodeId,
    view_n: consensus_engine::View,
}

impl Flat {
    pub fn new(view_n: consensus_engine::View, node_id: NodeId) -> Self {
        Self { node_id, view_n }
    }

    fn approve<Qc: Clone, TxId: Clone + Eq + Hash>(&self, _block: &Block<Qc, TxId>) -> Approval {
        // we still need to define how votes look like
        Approval
    }
}

#[async_trait::async_trait]
impl<Network, Fountain, VoteTally, TxId> Overlay<Network, Fountain, VoteTally, TxId> for Flat
where
    TxId: serde::de::DeserializeOwned + Clone + Eq + Hash + Send + Sync + 'static,
    Network: NetworkAdapter + Sync,
    Fountain: FountainCode + Sync,
    VoteTally: Tally + Sync,
    VoteTally::Vote: Serialize + DeserializeOwned + Send,
    VoteTally::Qc: Clone + DeserializeOwned + Send + Sync + 'static,
{
    fn new(view: &View, node: NodeId) -> Self {
        Flat::new(view.view_n, node)
    }

    async fn reconstruct_proposal_block(
        &self,
        view: &View,
        adapter: &Network,
        fountain: &Fountain,
    ) -> Result<Block<VoteTally::Qc, TxId>, FountainError> {
        assert_eq!(view.view_n, self.view_n, "view_n mismatch");
        let message_stream = adapter.proposal_chunks_stream(view.view_n).await;
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
        let block_bytes = block.as_bytes();
        let encoded_stream = fountain.encode(&block_bytes);
        encoded_stream
            .for_each_concurrent(None, |chunk| async move {
                let message = ProposalChunkMsg {
                    chunk,
                    view: view.view_n,
                };
                adapter.broadcast_block_chunk(view.view_n, message).await;
            })
            .await;
    }

    async fn approve_and_forward(
        &self,
        view: &View,
        block: &Block<VoteTally::Qc, TxId>,
        adapter: &Network,
        _tally: &VoteTally,
        _next_view: &View,
    ) -> Result<(), Box<dyn Error>> {
        assert_eq!(view.view_n, self.view_n, "view_n mismatch");
        // in the flat overlay, there's no need to wait for anyone before approving the block
        let approval = self.approve(block);
        adapter
            .send_vote(
                FLAT_COMMITTEE,
                view.view_n,
                VoteMsg {
                    vote: approval,
                    source: self.node_id,
                },
            )
            .await;
        Ok(())
    }

    async fn build_qc(&self, view: &View, adapter: &Network, tally: &VoteTally) -> VoteTally::Qc {
        assert_eq!(view.view_n, self.view_n, "view_n mismatch");

        // for now, let's pretend that consensus is reached as soon as the
        // block is approved by a share of the nodes
        let stream = Box::into_pin(adapter.votes_stream(FLAT_COMMITTEE, view.view_n).await);

        // Shadow the original binding so that it can't be directly accessed
        // ever again.
        // TODO: Remove the `try_into` call when tally is refactored to use with latest consensus engine types
        if let Ok((qc, _)) = tally.tally(view.view_n.try_into().unwrap(), stream).await {
            qc
        } else {
            unimplemented!("consensus not reached")
        }
    }
}
