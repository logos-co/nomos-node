use super::*;
use overwatch_rs::{overwatch::handle::OverwatchHandle, services::state::ServiceState};
use tokio::sync::broadcast::Receiver;

#[cfg(feature = "waku")]
pub mod waku;

#[cfg(feature = "libp2p")]
pub mod libp2p;

#[cfg(feature = "mock")]
pub mod mock;

#[async_trait::async_trait]
pub trait NetworkBackend {
    type Settings: Clone + Debug + Send + Sync + 'static;
    type State: ServiceState<Settings = Self::Settings> + Clone + Send + Sync;
    type Message: Debug + Send + Sync + 'static;
    type EventKind: Debug + Send + Sync + 'static;
    type NetworkEvent: Debug + Send + Sync + 'static;

    fn new(config: Self::Settings, overwatch_handle: OverwatchHandle) -> Self;
    async fn process(&self, msg: Self::Message);
    async fn subscribe(&mut self, event: Self::EventKind) -> Receiver<Self::NetworkEvent>;
}
