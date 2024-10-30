pub mod backends;
pub mod network;

use std::fmt::Debug;

use async_trait::async_trait;
use backends::MixBackend;
use futures::StreamExt;
use network::NetworkAdapter;
use nomos_core::wire;
use nomos_mix::message_blend::persistent_transmission::PersistentTransmissionSettings;
use nomos_mix::message_blend::{MessageBlendExt, MessageBlendSettings};
use nomos_mix::message_blend::{
    MessageBlendStreamIncomingMessage, MessageBlendStreamOutgoingMessage,
};
use nomos_network::NetworkService;
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    life_cycle::LifecycleMessage,
    relay::{Relay, RelayMessage},
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::sync::mpsc;

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
            service_state,
            mut backend,
            network_relay,
        } = self;
        let mix_config = service_state.settings_reader.get_updated_settings();

        let network_relay = network_relay.connect().await?;
        let network_adapter = Network::new(network_relay);

        // Spawn Message Blend and connect it to Persistent Transmission
        // A channel to listen to messages received from the [`MixBackend`]
        let incoming_message_stream = backend
            .listen_to_incoming_messages()
            .map(MessageBlendStreamIncomingMessage::Inbound);
        let inbound_relay = service_state
            .inbound_relay
            .map(|ServiceMessage::Mix(message)| {
                MessageBlendStreamIncomingMessage::Local(
                    wire::serialize(&message)
                        .expect("Message from internal services should not fail to serialize"),
                )
            });
        let mut incoming_blend_stream =
            tokio_stream::StreamExt::merge(incoming_message_stream, inbound_relay)
                .blend(mix_config.message_blend);

        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();
        loop {
            tokio::select! {
                // Already processed temporal message, publish to mixnet
                Some(msg) = incoming_blend_stream.next() => {
                    match msg {
                        MessageBlendStreamOutgoingMessage::Outbound(msg) => {
                            backend.publish(msg).await;
                        }
                        MessageBlendStreamOutgoingMessage::FullyUnwrapped(msg) => {
                            tracing::debug!("Broadcasting fully unwrapped message");
                            match wire::deserialize::<NetworkMessage<Network::BroadcastSettings>>(&msg) {
                                Ok(msg) => {
                                    network_adapter.broadcast(msg.message, msg.broadcast_settings).await;
                                },
                                _ => {
                                    tracing::error!("unrecognized message from mix backend");
                                }
                            }
                        }
                    }
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
    pub persistent_transmission: PersistentTransmissionSettings,
    pub message_blend: MessageBlendSettings,
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
