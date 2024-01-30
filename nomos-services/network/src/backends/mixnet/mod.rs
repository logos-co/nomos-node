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
    /// Broadcast a message through mixnet.
    ///
    /// A message will be split into multiple Sphinx packets, mixed through mixnet,
    /// reconstructed to the original message, and broadcasted to the entire network.
    BroadcastMessage(Box<[u8]>),
    /// Update config (topology, etc.)
    ///
    /// Once a new config is injected, internal mixnet components will work
    /// based on the new config.
    /// If a topology in the config has been changed, new network connections
    /// will be established in background to build the network topology.
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
