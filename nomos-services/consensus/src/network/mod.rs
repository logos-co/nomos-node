pub mod adapters;
pub mod messages;

// std
use bytes::Bytes;
// crates
use futures::Stream;
// internal
use crate::network::messages::{NewViewMsg, ProposalChunkMsg, TimeoutMsg, TimeoutQcMsg, VoteMsg};
use consensus_engine::{BlockId, Committee, View};
use nomos_network::backends::NetworkBackend;
use nomos_network::NetworkService;
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;

#[async_trait::async_trait]
pub trait NetworkAdapter {
    type Backend: NetworkBackend + 'static;
    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;
    async fn proposal_chunks_stream(
        &self,
        view: View,
    ) -> Box<dyn Stream<Item = Bytes> + Send + Sync + Unpin>;
    async fn broadcast_block_chunk(&self, chunk_msg: ProposalChunkMsg);
    async fn broadcast_timeout_qc(&self, timeout_qc_msg: TimeoutQcMsg);
    async fn timeout_stream(
        &self,
        committee: &Committee,
        view: View,
    ) -> Box<dyn Stream<Item = TimeoutMsg> + Send + Sync + Unpin>;
    async fn timeout_qc_stream(
        &self,
        view: View,
    ) -> Box<dyn Stream<Item = TimeoutQcMsg> + Send + Sync + Unpin>;
    async fn votes_stream(
        &self,
        committee: &Committee,
        view: View,
        proposal_id: BlockId,
    ) -> Box<dyn Stream<Item = VoteMsg> + Send + Unpin>;
    async fn new_view_stream(
        &self,
        committee: &Committee,
        view: View,
    ) -> Box<dyn Stream<Item = NewViewMsg> + Send + Unpin>;
    async fn send(&self, committee: &Committee, view: View, payload: Box<[u8]>, channel: &str);
}
