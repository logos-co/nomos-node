pub mod committees;
mod flat;

// std
use std::error::Error;
use std::hash::Hash;
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
pub trait Overlay<
    Network: NetworkAdapter,
    Fountain: FountainCode,
    VoteTally: Tally,
    TxId: Clone + Eq + Hash,
> where
    VoteTally::Qc: Clone,
{
    fn new(view: &View, node: NodeId) -> Self;

    async fn reconstruct_proposal_block(
        &self,
        view: &View,
        adapter: &Network,
        fountain: &Fountain,
    ) -> Result<Block<VoteTally::Qc, TxId>, FountainError>;
    async fn broadcast_block(
        &self,
        view: &View,
        block: Block<VoteTally::Qc, TxId>,
        adapter: &Network,
        fountain: &Fountain,
    );
    /// Different overlays might have different needs or the same overlay might
    /// require different steps depending on the node role
    /// For now let's put this responsibility on the overlay
    async fn approve_and_forward(
        &self,
        view: &View,
        block: &Block<VoteTally::Qc, TxId>,
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
    ) -> VoteTally::Qc;
}
