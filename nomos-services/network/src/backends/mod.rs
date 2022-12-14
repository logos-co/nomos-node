use super::*;
use overwatch_rs::services::state::ServiceState;
use tokio::sync::broadcast::Receiver;

#[cfg(feature = "waku")]
pub mod waku;

#[async_trait::async_trait]
pub trait NetworkBackend {
    type Config: Clone + Debug + Send + Sync + 'static;
    type State: ServiceState<Settings = Self::Config> + Clone;
    type Message: Debug + Send + Sync + 'static;
    type EventKind: Debug + Send + Sync + 'static;
    type NetworkEvent: Debug + Send + Sync + 'static;

    fn new(config: Self::Config) -> Self;
    async fn process(&self, msg: Self::Message);
    async fn subscribe(&mut self, event: Self::EventKind) -> Receiver<Self::NetworkEvent>;
}
