use super::{Approval, Block, NodeId, View};

#[allow(unused)]
mod committees;

pub use committees::Member;

// Dissamination overlay, tied to a specific view
#[async_trait::async_trait]
pub trait Overlay<'view> {
    fn new(view: &'view View, node: NodeId) -> Self;

    async fn reconstruct_proposal_block(&self) -> Block;
    async fn broadcast_block(&self, block: Block);
    async fn collect_approvals(&self, block: Block) -> tokio::sync::mpsc::Receiver<Approval>;
    async fn forward_approval(&self, approval: Approval);
}
