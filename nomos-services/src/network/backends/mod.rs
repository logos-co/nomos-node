use super::*;
use overwatch::services::state::ServiceState;
use tokio::sync::broadcast::Receiver;

pub trait NetworkBackend {
    type Config: Clone + Send + Sync + 'static;
    type State: ServiceState<Settings = Self::Config> + Clone;

    fn new(config: Self::Config) -> Self;
    fn broadcast(&self, msg: NetworkData);
    fn subscribe(&mut self, event: EventKind) -> Receiver<NetworkEvent>;
}
