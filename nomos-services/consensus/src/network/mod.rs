pub mod adapters;
pub mod messages;

// std
use bytes::Bytes;
// crates
use futures::Stream;
// internal
use crate::network::messages::{ProposalChunkMsg, TimeoutMsg, TimeoutQcMsg, VoteMsg};
use consensus_engine::{Committee, View};
use nomos_network::backends::NetworkBackend;
use nomos_network::NetworkService;
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
use serde::de::DeserializeOwned;
use serde::Serialize;

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
    async fn votes_stream<Vote: DeserializeOwned>(
        &self,
        committee: &Committee,
        view: View,
    ) -> Box<dyn Stream<Item = Vote> + Send + Unpin>;
    async fn send_vote<Vote: Serialize + Send>(
        &self,
        committee: &Committee,
        view: View,
        vote: VoteMsg,
    );
}
