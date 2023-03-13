use crate::network::messages::*;
use crate::overlay::committees::*;
use crate::overlay::*;
use crate::*;
use async_trait::async_trait;
use bytes::Bytes;
use futures::Stream;
use nomos_core::fountain::FountainError;
use nomos_core::fountain::{mock::MockFountain, FountainCode};
use nomos_core::vote::mock::{MockTally, MockTallySettings, QC};
use nomos_network::backends::NetworkBackend;
use nomos_network::NetworkService;
use overwatch_rs::services::relay::*;
use serde::de::DeserializeOwned;
use tokio::sync::broadcast::Receiver;

struct DummyOverlay;
struct DummyAdapter;
struct DummyBackend;

#[async_trait]
impl<N: NetworkAdapter + Sync, F: FountainCode + Sync> Overlay<N, F, MockTally> for DummyOverlay {
    fn new(_: &View, _: NodeId) -> Self {
        DummyOverlay
    }

    async fn reconstruct_proposal_block(
        &self,
        _view: &View,
        _adapter: &N,
        _fountain: &F,
    ) -> Result<Block, FountainError> {
        Ok(Block)
    }

    async fn broadcast_block(&self, _view: &View, _block: Block, _adapter: &N, _fountain: &F) {}

    async fn approve_and_forward(
        &self,
        _view: &View,
        _block: &Block,
        _adapter: &N,
        _vote_tally: &MockTally,
        _next_view: &View,
    ) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    async fn build_qc(&self, _view: &View, _adapter: &N, _vote_tally: &MockTally) -> QC {
        QC::Approved(0, 0)
    }
}

#[async_trait]
impl NetworkAdapter for DummyAdapter {
    type Backend = DummyBackend;
    async fn new(
        _: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        DummyAdapter
    }
    async fn proposal_chunks_stream(
        &self,
        _: Committee,
        _: &View,
    ) -> Box<dyn Stream<Item = Bytes> + Send + Sync + Unpin> {
        unimplemented!()
    }
    async fn broadcast_block_chunk(&self, _: Committee, _: &View, _: ProposalChunkMsg) {
        unimplemented!()
    }
    async fn votes_stream<Vote: DeserializeOwned>(
        &self,
        _committee: Committee,
        _view: &View,
    ) -> Box<dyn Stream<Item = Vote> + Send> {
        unimplemented!()
    }
    async fn forward_approval<Vote: Serialize + Send>(
        &self,
        _committee: Committee,
        _view: &View,
        _approval: ApprovalMsg<Vote>,
    ) {
        unimplemented!()
    }
}

#[async_trait]
impl NetworkBackend for DummyBackend {
    type Settings = ();
    type State = NoState<()>;
    type Message = ();
    type EventKind = ();
    type NetworkEvent = ();

    fn new(_config: Self::Settings) -> Self {
        Self
    }
    async fn process(&self, _: Self::Message) {}
    async fn subscribe(&mut self, _: Self::EventKind) -> Receiver<Self::NetworkEvent> {
        unimplemented!()
    }
}

#[tokio::test]
async fn test_single_round_non_leader() {
    let view = View {
        seed: [0; 32],
        staking_keys: BTreeMap::new(),
        view_n: 0,
    };
    let mock_tally = MockTally::new(MockTallySettings { threshold: 0 });
    let (_, next_view) = view
        .resolve_non_leader::<DummyAdapter, DummyOverlay, MockFountain, MockTally>(
            [0; 32],
            &DummyAdapter,
            &MockFountain,
            &mock_tally,
        )
        .await
        .unwrap();
    assert!(next_view.view_n == 1);
}
