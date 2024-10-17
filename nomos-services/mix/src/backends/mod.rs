#[cfg(feature = "libp2p")]
pub mod libp2p;

use std::{fmt::Debug, pin::Pin};

use futures::Stream;
use overwatch_rs::{overwatch::handle::OverwatchHandle, services::state::ServiceState};

#[async_trait::async_trait]
pub trait MixBackend {
    type Settings: Clone + Debug + Send + Sync + 'static;
    type State: ServiceState<Settings = Self::Settings> + Clone + Send + Sync;
    type Message: Debug + Send + Sync + 'static;
    type EventKind: Debug + Send + Sync + 'static;
    type NetworkEvent: Debug + Send + Sync + 'static;

    fn new(config: Self::Settings, overwatch_handle: OverwatchHandle) -> Self;
    async fn process(&self, msg: Self::Message);
    async fn subscribe(
        &mut self,
        event: Self::EventKind,
    ) -> Pin<Box<dyn Stream<Item = Self::NetworkEvent> + Send>>;
}
