pub mod committees;
mod flat;

// std
use std::error::Error;
// crates
// internal
use super::{Approval, NodeId, View};
use crate::network::NetworkAdapter;
pub use committees::Member;
use nomos_core::block::Block;
use nomos_core::fountain::{FountainCode, FountainError};
use nomos_core::vote::Tally;

/// Dissemination overlay, tied to a specific view
#[async_trait::async_trait]
pub trait Overlay<Network: NetworkAdapter, Fountain: FountainCode, VoteTally: Tally> {
    fn new(view: &View, node: NodeId) -> Self;

    async fn reconstruct_proposal_block(
        &self,
        view: &View,
        adapter: &Network,
        fountain: &Fountain,
    ) -> Result<Block, FountainError>;
    async fn broadcast_block(
        &self,
        view: &View,
        block: Block,
        adapter: &Network,
        fountain: &Fountain,
    );
    /// Different overlays might have different needs or the same overlay might
    /// require different steps depending on the node role
    /// For now let's put this responsibility on the overlay
    async fn approve_and_forward(
        &self,
        view: &View,
        block: &Block,
        adapter: &Network,
        vote_tally: &VoteTally,
        next_view: &View,
    ) -> Result<(), Box<dyn Error>>;
    /// Wait for consensus on a block
    async fn build_qc(
        &self,
        view: &View,
        adapter: &Network,
        vote_tally: &VoteTally,
    ) -> VoteTally::Outcome;
}
