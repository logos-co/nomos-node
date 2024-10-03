// std
use std::fmt::Debug;
use std::marker::PhantomData;
// crates
use tokio::sync::oneshot;
// internal
use crate::adapters::network::DispersalNetworkAdapter;
use crate::backend::DispersalBackend;
use nomos_da_network_core::{PeerId, SubnetworkId};
use nomos_da_network_service::backends::libp2p::executor::DaNetworkExecutorBackend;
use nomos_da_network_service::NetworkService;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::relay::{Relay, RelayMessage};
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use overwatch_rs::DynError;
use subnetworks_assignations::MembershipHandler;

mod adapters;
pub mod backend;

const DA_DISPERSAL_TAG: ServiceId = "DA-Encoder";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Foo bar {0}")]
    Encoding(String),
}

#[derive(Debug)]
pub enum DaDispersalMsg {
    Disperse {
        blob: Vec<u8>,
        reply_channel: oneshot::Sender<Result<(), Error>>,
    },
}

impl RelayMessage for DaDispersalMsg {}

#[derive(Clone)]
pub struct DispersalServiceSettings<BackendSettings> {
    backend: BackendSettings,
}

pub struct DispersalService<Backend, NetworkAdapter, Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
    Backend: DispersalBackend<Adapter = NetworkAdapter>,
    Backend::Settings: Clone,
    NetworkAdapter: DispersalNetworkAdapter,
{
    service_state: ServiceStateHandle<Self>,
    network_relay: Relay<NetworkAdapter::NetworkService>,
    _backend: PhantomData<Backend>,
    _network_adapter: PhantomData<NetworkAdapter>,
}

impl<Backend, NetworkAdapter, Membership> ServiceData
    for DispersalService<Backend, NetworkAdapter, Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
    Backend: DispersalBackend<Adapter = NetworkAdapter>,
    Backend::Settings: Clone,
    NetworkAdapter: DispersalNetworkAdapter,
{
    const SERVICE_ID: ServiceId = DA_DISPERSAL_TAG;
    type Settings = DispersalServiceSettings<Backend::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = DaDispersalMsg;
}

#[async_trait::async_trait]
impl<Backend, NetworkAdapter, Membership> ServiceCore
    for DispersalService<Backend, NetworkAdapter, Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
    Backend: DispersalBackend<Adapter = NetworkAdapter> + Send,
    Backend::Settings: Clone + Send + Sync,
    NetworkAdapter: DispersalNetworkAdapter<SubnetworkId = Membership::NetworkId> + Send,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let network_relay = service_state.overwatch_handle.relay();
        Ok(Self {
            service_state,
            network_relay,
            _backend: Default::default(),
            _network_adapter: Default::default(),
        })
    }

    async fn run(self) -> Result<(), DynError> {
        let Self {
            network_relay,
            mut service_state,
            ..
        } = self;
        let DispersalServiceSettings {
            backend: backend_settings,
        } = service_state.settings_reader.get_updated_settings();
        let network_relay = network_relay.connect().await?;
        let network_adapter = NetworkAdapter::new(network_relay);
        let backend = Backend::init(backend_settings, network_adapter);
        // let dispersal_events_stream = network_adapter.dispersal_events_stream().await?;
        let mut inbound_relay = &mut service_state.inbound_relay;
        while let Some(incoming_message) = inbound_relay.recv().await {}
        Ok(())
    }
}
