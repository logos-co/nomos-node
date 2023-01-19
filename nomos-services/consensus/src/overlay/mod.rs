#[allow(unused)]
mod committees;

// std
// crates
// internal
use super::{Approval, NodeId, View};
use crate::network::NetworkAdapter;
pub use committees::Member;
use nomos_core::block::Block;
use nomos_core::fountain::{FountainCode, FountainError};

/// Dissemination overlay, tied to a specific view
#[async_trait::async_trait]
pub trait Overlay<'view, Network: NetworkAdapter, Fountain: FountainCode> {
    fn new(view: &'view View, node: NodeId) -> Self;

    async fn reconstruct_proposal_block(
        &self,
        adapter: &Network,
        fountain: &Fountain,
    ) -> Result<Block, FountainError>;
    async fn broadcast_block(&self, block: Block, adapter: &Network, fountain: &Fountain);
    async fn collect_approvals(
        &self,
        block: Block,
        adapter: &Network,
    ) -> tokio::sync::mpsc::Receiver<Approval>;
    async fn forward_approval(&self, approval: Approval, adapter: &Network);
}
