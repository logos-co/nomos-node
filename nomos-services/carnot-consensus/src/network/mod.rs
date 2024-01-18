pub mod adapters;
pub mod messages;

// std
// crates
use futures::{Future, Stream};
// internal
use crate::network::messages::{
    NetworkMessage, NewViewMsg, ProposalMsg, TimeoutMsg, TimeoutQcMsg, VoteMsg,
};
use carnot_engine::{BlockId, Committee, View};
use nomos_network::backends::NetworkBackend;
use nomos_network::NetworkService;
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;

pub trait NetworkAdapter {
    type Backend: NetworkBackend + 'static;
    fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> impl Future<Output = Self> + Send
    where
        Self: Sized;
    fn proposal_chunks_stream(
        &self,
        view: View,
    ) -> impl Future<Output = impl Stream<Item = ProposalMsg> + Send + Sync + Unpin + 'static> + Send;
    fn broadcast(&self, message: NetworkMessage) -> impl Future<Output = ()> + Send;
    fn timeout_stream(
        &self,
        committee: &Committee,
        view: View,
    ) -> impl Future<Output = impl Stream<Item = TimeoutMsg> + Send + Sync + Unpin + 'static> + Send;
    fn timeout_qc_stream(
        &self,
        view: View,
    ) -> impl Future<Output = impl Stream<Item = TimeoutQcMsg> + Send + Sync + Unpin + 'static> + Send;
    fn votes_stream(
        &self,
        committee: &Committee,
        view: View,
        proposal_id: BlockId,
    ) -> impl Future<Output = impl Stream<Item = VoteMsg> + Send + Sync + Unpin + 'static> + Send;
    fn new_view_stream(
        &self,
        committee: &Committee,
        view: View,
    ) -> impl Future<Output = impl Stream<Item = NewViewMsg> + Send + Sync + Unpin + 'static> + Send;
    fn send(
        &self,
        message: NetworkMessage,
        committee: &Committee,
    ) -> impl Future<Output = ()> + Send;
}
