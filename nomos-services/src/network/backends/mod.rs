use super::*;
use overwatch::services::state::ServiceState;
use tokio::sync::broadcast::Receiver;

mod waku;

pub trait NetworkBackend {
    type Config: Clone + Send + Sync + 'static;
    type State: ServiceState<Settings = Self::Config> + Clone;
    type Message: Debug + Send + Sync + 'static;

    fn new(config: Self::Config) -> Self;
    fn broadcast(&self, msg: Self::Message);
    fn subscribe(&mut self, event: EventKind) -> Receiver<NetworkEvent<Self::Message>>;
}
