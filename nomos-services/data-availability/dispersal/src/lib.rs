use std::{fmt::Debug, marker::PhantomData};

use nomos_core::da::blob::metadata;
use nomos_da_network_core::{PeerId, SubnetworkId};
use overwatch::{
    services::{
        relay::{Relay, RelayMessage},
        state::{NoOperator, NoState},
        ServiceCore, ServiceData, ServiceId,
    },
    DynError, OpaqueServiceStateHandle,
};
use serde::{Deserialize, Serialize};
use subnetworks_assignations::MembershipHandler;
use tokio::sync::oneshot;
use tracing::error;

use crate::{
    adapters::{mempool::DaMempoolAdapter, network::DispersalNetworkAdapter},
    backend::DispersalBackend,
};

pub mod adapters;
pub mod backend;

const DA_DISPERSAL_TAG: ServiceId = "DA-Encoder";

#[derive(Debug)]
pub enum DaDispersalMsg<Metadata> {
    Disperse {
        data: Vec<u8>,
        metadata: Metadata,
        reply_channel: oneshot::Sender<Result<(), DynError>>,
    },
}

impl<Metadata: 'static> RelayMessage for DaDispersalMsg<Metadata> {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DispersalServiceSettings<BackendSettings> {
    pub backend: BackendSettings,
}

pub struct DispersalService<Backend, NetworkAdapter, MempoolAdapter, Membership, Metadata>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
    Backend: DispersalBackend<NetworkAdapter = NetworkAdapter, Metadata = Metadata>,
    Backend::Settings: Clone,
    NetworkAdapter: DispersalNetworkAdapter,
    MempoolAdapter: DaMempoolAdapter,
    Metadata: metadata::Metadata + Debug + 'static,
{
    service_state: OpaqueServiceStateHandle<Self>,
    network_relay: Relay<NetworkAdapter::NetworkService>,
    mempool_relay: Relay<MempoolAdapter::MempoolService>,
    _backend: PhantomData<Backend>,
}

impl<Backend, NetworkAdapter, MempoolAdapter, Membership, Metadata> ServiceData
    for DispersalService<Backend, NetworkAdapter, MempoolAdapter, Membership, Metadata>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
    Backend: DispersalBackend<NetworkAdapter = NetworkAdapter, Metadata = Metadata>,
    Backend::Settings: Clone,
    NetworkAdapter: DispersalNetworkAdapter,
    MempoolAdapter: DaMempoolAdapter,
    Metadata: metadata::Metadata + Debug + 'static,
{
    const SERVICE_ID: ServiceId = DA_DISPERSAL_TAG;
    type Settings = DispersalServiceSettings<Backend::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State, Self::Settings>;
    type Message = DaDispersalMsg<Metadata>;
}

#[async_trait::async_trait]
impl<Backend, NetworkAdapter, MempoolAdapter, Membership, Metadata> ServiceCore
    for DispersalService<Backend, NetworkAdapter, MempoolAdapter, Membership, Metadata>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
    Backend: DispersalBackend<
            NetworkAdapter = NetworkAdapter,
            MempoolAdapter = MempoolAdapter,
            Metadata = Metadata,
        > + Send
        + Sync,
    Backend::Settings: Clone + Send + Sync,
    NetworkAdapter: DispersalNetworkAdapter<SubnetworkId = Membership::NetworkId> + Send,
    <NetworkAdapter::NetworkService as ServiceData>::Message: 'static,
    MempoolAdapter: DaMempoolAdapter,
    <MempoolAdapter::MempoolService as ServiceData>::Message: 'static,
    Metadata: metadata::Metadata + Debug + Send + 'static,
{
    fn init(
        service_state: OpaqueServiceStateHandle<Self>,
        _init_state: Self::State,
    ) -> Result<Self, DynError> {
        let network_relay = service_state.overwatch_handle.relay();
        let mempool_relay = service_state.overwatch_handle.relay();
        Ok(Self {
            service_state,
            network_relay,
            mempool_relay,
            _backend: PhantomData,
        })
    }

    async fn run(self) -> Result<(), DynError> {
        let Self {
            network_relay,
            mempool_relay,
            service_state,
            ..
        } = self;

        let DispersalServiceSettings {
            backend: backend_settings,
        } = service_state.settings_reader.get_updated_settings();
        let network_relay = network_relay.connect().await?;
        let network_adapter = NetworkAdapter::new(network_relay);
        let mempool_relay = mempool_relay.connect().await?;
        let mempool_adapter = MempoolAdapter::new(mempool_relay);
        let backend = Backend::init(backend_settings, network_adapter, mempool_adapter);
        let mut inbound_relay = service_state.inbound_relay;
        while let Some(dispersal_msg) = inbound_relay.recv().await {
            match dispersal_msg {
                DaDispersalMsg::Disperse {
                    data,
                    metadata,
                    reply_channel,
                } => {
                    if let Err(Err(e)) =
                        reply_channel.send(backend.process_dispersal(data, metadata).await)
                    {
                        error!("Error forwarding dispersal response: {e}");
                    }
                }
            }
        }

        Ok(())
    }
}
