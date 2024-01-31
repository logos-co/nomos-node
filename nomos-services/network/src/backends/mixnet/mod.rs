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
        // TODO: One important task that should be spawned is
        // subscribing NewEntropy events that will be emitted from the consensus service soon.
        // so that new topology can be built internally.
        // In the mixnet spec, the robustness layer is responsible for this task.
        // We can implement the robustness layer in the mixnet-specific crate,
        // that we're going to define at the root of the project.
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
