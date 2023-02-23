pub mod adapters;
pub mod messages;

// std
use bytes::Bytes;
// crates
use futures::Stream;
// internal
use crate::network::messages::{ApprovalMsg, ProposalChunkMsg};
use crate::overlay::committees::Committee;
use crate::View;
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
        committee: Committee,
        view: &View,
    ) -> Box<dyn Stream<Item = Bytes> + Send + Sync + Unpin>;
    async fn broadcast_block_chunk(
        &self,
        committee: Committee,
        view: &View,
        chunk_msg: ProposalChunkMsg,
    );
    async fn votes_stream<Vote: DeserializeOwned>(
        &self,
        committee: Committee,
        view: &View,
    ) -> Box<dyn Stream<Item = Vote> + Send>;
    async fn forward_approval<Vote: Serialize + Send>(
        &self,
        committee: Committee,
        view: &View,
        approval: ApprovalMsg<Vote>,
    );
}
