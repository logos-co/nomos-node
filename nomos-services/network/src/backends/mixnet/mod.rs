// internal
use super::NetworkBackend;
use nomos_libp2p::gossipsub;
// crates
use overwatch_rs::{overwatch::handle::OverwatchHandle, services::state::NoState};
use tokio::sync::broadcast;

/// A Mixnet network backend broadcasts messages to the network with mixing packets through mixnet,
/// and receives messages broadcasted from the network.
pub struct MixnetNetworkBackend {}

#[async_trait::async_trait]
impl NetworkBackend for MixnetNetworkBackend {
    type Settings = MixnetConfig;
    type State = NoState<MixnetConfig>;
    type Message = Command;
    type EventKind = EventKind;
    type NetworkEvent = NetworkEvent;

    fn new(_config: Self::Settings, _overwatch_handle: OverwatchHandle) -> Self {
        todo!()
    }

    async fn process(&self, _msg: Self::Message) {
        todo!()
    }

    async fn subscribe(
        &mut self,
        _kind: Self::EventKind,
    ) -> broadcast::Receiver<Self::NetworkEvent> {
        todo!()
    }
}

#[derive(Clone, Debug)]
pub struct MixnetConfig {}

#[derive(Debug)]
pub enum Command {
    /// Broadcast a message through mixnet
    BroadcastMessage(Box<[u8]>),
    /// Update config (topology, etc.)
    UpdateConfig(MixnetConfig),
}

#[derive(Debug)]
pub enum EventKind {
    MessageReceived,
}

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    /// Received a message broadcasted
    MessageReceived(gossipsub::Message),
}
