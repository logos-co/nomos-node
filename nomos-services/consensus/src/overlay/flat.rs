use std::collections::HashSet;
// std
use std::error::Error;
// crates
use futures::StreamExt;
// internal
use super::*;
use crate::network::messages::{ApprovalMsg, ProposalChunkMsg};
use crate::network::NetworkAdapter;
use crate::overlay::committees::Committee;

const DEFAULT_THRESHOLD: Threshold = Threshold::new(2, 3);
const FLAT_COMMITTEE: Committee = Committee::root();
/// The share of nodes that need to approve a block for it to be valid
/// expressed as a fraction of the total number of nodes
#[derive(Copy, Clone, Debug)]
pub struct Threshold {
    num: u64,
    den: u64,
}

impl Threshold {
    pub const fn new(num: u64, den: u64) -> Self {
        Self { num, den }
    }
}

/// A flat overlay, everyone is in the same committee.
/// As far as the API is concerned, this should be equivalent to any other
/// overlay and far simpler to implement.
/// For this reason, this might act as a 'reference' overlay for testing.
pub struct Flat {
    // TODO: this should be a const param, but we can't do that yet
    threshold: Threshold,
    node_id: NodeId,
    view_n: u64,
}

impl Flat {
    pub fn new(view_n: u64, node_id: NodeId) -> Self {
        Self {
            threshold: DEFAULT_THRESHOLD,
            node_id,
            view_n,
        }
    }

    fn approve(&self, _block: &Block) -> Approval {
        // we still need to define how votes look like
        todo!()
    }
}

#[async_trait::async_trait]
impl<Network: NetworkAdapter + Sync, Fountain: FountainCode + Sync> Overlay<Network, Fountain>
    for Flat
{
    fn new(view: &View, node: NodeId) -> Self {
        Flat::new(view.view_n, node)
    }

    async fn reconstruct_proposal_block(
        &self,
        view: &View,
        adapter: &Network,
        fountain: &Fountain,
    ) -> Result<Block, FountainError> {
        assert_eq!(view.view_n, self.view_n, "view_n mismatch");
        let message_stream = adapter.proposal_chunks_stream(FLAT_COMMITTEE, view).await;
        fountain.decode(message_stream).await.map(Block::from_bytes)
    }

    async fn broadcast_block(
        &self,
        view: &View,
        block: Block,
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
        block: &Block,
        adapter: &Network,
        _next_view: &View,
    ) -> Result<(), Box<dyn Error>> {
        assert_eq!(view.view_n, self.view_n, "view_n mismatch");
        // in the flat overlay, there's no need to wait for anyone before approving the block
        let approval = self.approve(block);
        adapter
            .forward_approval(
                FLAT_COMMITTEE,
                view,
                ApprovalMsg {
                    approval,
                    source: self.node_id,
                },
            )
            .await;
        Ok(())
    }

    async fn build_qc(&self, view: &View, adapter: &Network) -> Approval {
        assert_eq!(view.view_n, self.view_n, "view_n mismatch");

        // for now, let's pretend that consensus is reached as soon as the
        // block is approved by a share of the nodes
        let mut approvals = HashSet::new();
        let mut stream = Box::into_pin(adapter.approvals_stream(FLAT_COMMITTEE, view).await);

        // Shadow the original binding so that it can't be directly accessed
        // ever again.
        while let Some(approval) = stream.next().await {
            // insert the approval in the map to deduplicate
            // TODO: validate approval
            approvals.insert(approval);
            // ceil(num/den * n)
            let threshold =
                (self.threshold.num * view.staking_keys.len() as u64 + self.threshold.den - 1)
                    / self.threshold.den;
            if approvals.len() as u64 >= threshold {
                // consensus reached
                // FIXME: build a real QC
                return Approval;
            }
        }
        unimplemented!("consensus not reached")
    }
}
