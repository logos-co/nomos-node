use super::{Approval, Block, NodeId, View};

#[allow(unused)]
mod committees;

use crate::network::NetworkAdapter;
pub use committees::Member;

// Dissamination overlay, tied to a specific view
#[async_trait::async_trait]
pub trait Overlay<'view, Network: NetworkAdapter> {
    fn new(view: &'view View, node: NodeId) -> Self;

    async fn reconstruct_proposal_block(&self, adapter: &Network) -> Block;
    async fn broadcast_block(&self, block: Block, adapter: &Network);
    async fn collect_approvals(
        &self,
        block: Block,
        adapter: &Network,
    ) -> tokio::sync::mpsc::Receiver<Approval>;
    async fn forward_approval(&self, approval: Approval, adapter: &Network);
}
