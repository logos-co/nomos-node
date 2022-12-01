use nomos_network::backends::NetworkBackend;
use nomos_network::NetworkService;
use overwatch_rs::services::relay::Relay;

pub trait NetworkAdapter {
    type Backend: NetworkBackend + Send + Sync + 'static;
    fn new(network_service: Relay<NetworkService<Self::Backend>>) -> Self;
}
