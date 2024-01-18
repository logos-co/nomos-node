use super::*;
use futures::Future;
use overwatch_rs::{overwatch::handle::OverwatchHandle, services::state::ServiceState};
use tokio::sync::broadcast::Receiver;

#[cfg(feature = "libp2p")]
pub mod libp2p;

#[cfg(feature = "mock")]
pub mod mock;

pub trait NetworkBackend: Send {
    type Settings: Clone + Debug + Send + Sync + 'static;
    type State: ServiceState<Settings = Self::Settings> + Clone + Send + Sync;
    type Message: Debug + Send + Sync + 'static;
    type EventKind: Debug + Send + Sync + 'static;
    type NetworkEvent: Debug + Send + Sync + 'static;

    fn new(config: Self::Settings, overwatch_handle: OverwatchHandle) -> Self;
    fn process(&self, msg: Self::Message) -> impl Future<Output = ()> + Send;
    fn subscribe(
        &mut self,
        event: Self::EventKind,
    ) -> impl Future<Output = Receiver<Self::NetworkEvent>> + Send;
}
