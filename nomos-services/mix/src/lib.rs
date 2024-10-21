pub mod backends;
pub mod network;

use std::fmt::Debug;

use async_trait::async_trait;
use backends::MixBackend;
use futures::StreamExt;
use network::NetworkAdapter;
use nomos_core::wire;
use nomos_network::NetworkService;
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    life_cycle::LifecycleMessage,
    relay::{Relay, RelayMessage},
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// A mix service that sends messages to the mix network
/// and broadcasts fully unwrapped messages through the [`NetworkService`].
///
/// The mix backend and the network adapter are generic types that are independent with each other.
/// For example, the mix backend can use the libp2p network stack, while the network adapter can use the other network backend.
pub struct MixService<Backend, Network>
where
    Backend: MixBackend + 'static,
    Backend::Settings: Clone + Debug,
    Network: NetworkAdapter,
    Network::BroadcastSettings: Clone + Debug + Serialize + DeserializeOwned,
{
    backend: Backend,
    service_state: ServiceStateHandle<Self>,
    network_relay: Relay<NetworkService<Network::Backend>>,
}

impl<Backend, Network> ServiceData for MixService<Backend, Network>
where
    Backend: MixBackend + 'static,
    Backend::Settings: Clone,
    Network: NetworkAdapter,
    Network::BroadcastSettings: Clone + Debug + Serialize + DeserializeOwned,
{
    const SERVICE_ID: ServiceId = "Mix";
    type Settings = MixConfig<Backend::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = ServiceMessage<Network::BroadcastSettings>;
}

#[async_trait]
impl<Backend, Network> ServiceCore for MixService<Backend, Network>
where
    Backend: MixBackend + Send + 'static,
    Backend::Settings: Clone,
    Network: NetworkAdapter + Send + Sync + 'static,
    Network::BroadcastSettings:
        Clone + Debug + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        let network_relay = service_state.overwatch_handle.relay();
        Ok(Self {
            backend: <Backend as MixBackend>::new(
                service_state.settings_reader.get_updated_settings().backend,
                service_state.overwatch_handle.clone(),
            ),
            service_state,
            network_relay,
        })
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
        let Self {
            mut service_state,
            mut backend,
            network_relay,
        } = self;

        let network_relay = network_relay.connect().await?;
        let network_adapter = Network::new(network_relay);

        // A channel to listen to fully unwrapped messages returned from the [`MixBackend`]
        // if this node is the last node that can unwrap the data message.
        let mut fully_unwrapped_messages_receiver = backend.listen_to_fully_unwrapped_messages();

        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();
        loop {
            tokio::select! {
                Some(msg) = fully_unwrapped_messages_receiver.next() => {
                    tracing::debug!("Received fully unwrapped message from mix backend. Broadcasting it to the network service..");
                    // Deserialize the received message to get the topic for broadcasting
                    match wire::deserialize::<NetworkMessage<Network::BroadcastSettings>>(&msg) {
                        Ok(msg) => {
                            network_adapter.broadcast(msg.message, msg.broadcast_settings).await;
                        },
                        _ => tracing::error!("unrecognized message from mix backend")
                    }
                }
                Some(msg) = service_state.inbound_relay.recv() => {
                    Self::handle_service_message(msg, &mut backend).await;
                }
                Some(msg) = lifecycle_stream.next() => {
                    if Self::should_stop_service(msg).await {
                        break;
                    }
                }
            }
        }

        Ok(())
    }
}

impl<Backend, Network> MixService<Backend, Network>
where
    Backend: MixBackend + Send + 'static,
    Backend::Settings: Clone,
    Network: NetworkAdapter,
    Network::BroadcastSettings: Clone + Debug + Serialize + DeserializeOwned,
{
    async fn handle_service_message(
        msg: ServiceMessage<Network::BroadcastSettings>,
        backend: &mut Backend,
    ) {
        match msg {
            ServiceMessage::Mix(msg) => {
                // split sending in two steps to help the compiler understand we do not
                // need to hold an instance of &I (which is not send) across an await point
                let _send = backend.mix(wire::serialize(&msg).unwrap());
                _send.await
            }
        }
    }

    async fn should_stop_service(msg: LifecycleMessage) -> bool {
        match msg {
            LifecycleMessage::Kill => true,
            LifecycleMessage::Shutdown(signal_sender) => {
                // TODO: Maybe add a call to backend to handle this. Maybe trying to save unprocessed messages?
                if signal_sender.send(()).is_err() {
                    tracing::error!(
                        "Error sending successful shutdown signal from service {}",
                        Self::SERVICE_ID
                    );
                }
                true
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixConfig<BackendSettings> {
    pub backend: BackendSettings,
}

/// A message that is handled by [`MixService`].
#[derive(Debug)]
pub enum ServiceMessage<BroadcastSettings> {
    /// To send a message to the mix network and eventually broadcast it to the [`NetworkService`].
    Mix(NetworkMessage<BroadcastSettings>),
}

impl<BroadcastSettings> RelayMessage for ServiceMessage<BroadcastSettings> where
    BroadcastSettings: 'static
{
}

/// A message that is sent to the mix network.
/// To eventually broadcast the message to the network service,
/// [`BroadcastSettings`] must be included in the [`NetworkMessage`].
/// [`BroadcastSettings`] is a generic type defined by [`NetworkAdapter`].
#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkMessage<BroadcastSettings> {
    pub message: Vec<u8>,
    pub broadcast_settings: BroadcastSettings,
}
