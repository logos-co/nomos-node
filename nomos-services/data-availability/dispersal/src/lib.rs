// std

// crates

use futures::Stream;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::DynError;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::pin::Pin;
// internal
use nomos_da_network_core::{PeerId, SubnetworkId};
use nomos_da_network_service::backends::libp2p::executor::{
    DaNetworkEvent, DaNetworkEventKind, DaNetworkExecutorBackend,
};
use nomos_da_network_service::{DaNetworkMsg, NetworkService};
use overwatch_rs::services::relay::{OutboundRelay, Relay, RelayMessage};
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use subnetworks_assignations::MembershipHandler;
use tokio::sync::oneshot;
use tokio::sync::oneshot::error::RecvError;

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

pub struct DispersalService<Backend, Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
{
    service_state: ServiceStateHandle<Self>,
    network_relay: Relay<NetworkService<DaNetworkExecutorBackend<Membership>>>,
    _backend: PhantomData<Backend>,
}

impl<Backend, Membership> ServiceData for DispersalService<Backend, Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
{
    const SERVICE_ID: ServiceId = DA_DISPERSAL_TAG;
    type Settings = ();
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = DaDispersalMsg;
}

#[async_trait::async_trait]
impl<Backend, Membership> ServiceCore for DispersalService<Backend, Membership>
where
    Backend: Send,
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let network_relay = service_state
            .overwatch_handle
            .relay::<NetworkService<DaNetworkExecutorBackend<Membership>>>();
        Ok(Self {
            service_state,
            network_relay,
            _backend: Default::default(),
        })
    }

    async fn run(self) -> Result<(), DynError> {
        let Self { network_relay, .. } = self;
        let network_relay = network_relay.connect().await?;
        let dispersal_events_stream = dispersal_events_subscribe_stream(network_relay).await?;
        // loop {
        //     tokio::select! {
        //
        //     }
        // }
        Ok(())
    }
}

async fn dispersal_events_subscribe_stream<Membership>(
    network_relay: OutboundRelay<DaNetworkMsg<DaNetworkExecutorBackend<Membership>>>,
) -> Result<Pin<Box<dyn Stream<Item = DaNetworkEvent> + Send>>, DynError>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
{
    let (sender, receiver) = oneshot::channel();
    network_relay
        .send(DaNetworkMsg::Subscribe {
            kind: DaNetworkEventKind::Dispersal,
            sender,
        })
        .await
        .map_err(|(e, _)| Box::new(e) as DynError)?;
    receiver.await.map_err(|e| Box::new(e) as DynError)
}
