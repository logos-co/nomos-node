pub mod adapters;
mod messages;

// std
use bytes::Bytes;
// crates
use futures::Stream;
// internal
use crate::network::messages::{ApprovalMsg, ProposalChunkMsg};
use crate::{Approval, View};
use nomos_network::backends::NetworkBackend;
use nomos_network::NetworkService;
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;

#[async_trait::async_trait]
pub trait NetworkAdapter {
    type Backend: NetworkBackend + Send + Sync + 'static;
    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;
    async fn proposal_chunks_stream(&self) -> Box<dyn Stream<Item = Bytes>>;
    async fn broadcast_block_chunk(&self, view: View, chunk_msg: ProposalChunkMsg);
    async fn approvals_stream(&self) -> Box<dyn Stream<Item = Approval>>;
    async fn forward_approval(&self, approval: ApprovalMsg);
}
