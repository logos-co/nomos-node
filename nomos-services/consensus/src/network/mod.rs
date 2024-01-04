pub mod adapters;
pub mod messages;

// std
// crates
use futures::Stream;
// internal
use crate::network::messages::{
    NetworkMessage, NewViewMsg, ProposalMsg, TimeoutMsg, TimeoutQcMsg, VoteMsg,
};
use carnot_engine::{BlockId, Committee, View};
use nomos_network::backends::NetworkBackend;
use nomos_network::NetworkService;
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;

type BoxedStream<T> = Box<dyn Stream<Item = T> + Send + Sync + Unpin>;

#[async_trait::async_trait]
pub trait NetworkAdapter {
    type Backend: NetworkBackend + 'static;
    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self;
    async fn proposal_chunks_stream(
        &self,
        view: View,
    ) -> Box<dyn Stream<Item = ProposalMsg> + Send + Sync + Unpin>;
    async fn broadcast(&self, message: NetworkMessage);
    async fn timeout_stream(&self, committee: &Committee, view: View) -> BoxedStream<TimeoutMsg>;
    async fn timeout_qc_stream(&self, view: View) -> BoxedStream<TimeoutQcMsg>;
    async fn votes_stream(
        &self,
        committee: &Committee,
        view: View,
        proposal_id: BlockId,
    ) -> BoxedStream<VoteMsg>;
    async fn new_view_stream(&self, committee: &Committee, view: View) -> BoxedStream<NewViewMsg>;
    async fn send(&self, message: NetworkMessage, committee: &Committee);
}
