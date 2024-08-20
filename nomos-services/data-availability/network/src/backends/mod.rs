pub mod libp2p;
pub mod mock;

use super::*;
use futures::Stream;
use overwatch_rs::{overwatch::handle::OverwatchHandle, services::state::ServiceState};
use std::pin::Pin;

#[async_trait::async_trait]
pub trait NetworkBackend {
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
