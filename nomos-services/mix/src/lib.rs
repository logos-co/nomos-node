pub mod backends;
pub mod network;

use std::fmt::Debug;

use async_trait::async_trait;
use backends::MixBackend;
use futures::StreamExt;
use network::NetworkAdapter;
use nomos_core::wire;
use nomos_mix::{
    membership::{Membership, Node},
    message_blend::{CryptographicProcessorSettings, MessageBlend, MessageBlendSettings},
    persistent_transmission::{persistent_transmission, PersistentTransmissionSettings},
};
use nomos_mix_message::MessageBuilder;
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
    membership: Membership,
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
        let mix_config = service_state.settings_reader.get_updated_settings();
        let membership = mix_config.membership();
        let network_relay = service_state.overwatch_handle.relay();
        Ok(Self {
            backend: <Backend as MixBackend>::new(
                service_state.settings_reader.get_updated_settings().backend,
                service_state.overwatch_handle.clone(),
                membership.clone(),
            ),
            service_state,
            network_relay,
            membership,
        })
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
        let Self {
            mut service_state,
            mut backend,
            network_relay,
            membership,
        } = self;
        let mix_config = service_state.settings_reader.get_updated_settings();

        let network_relay = network_relay.connect().await?;
        let network_adapter = Network::new(network_relay);

        // Create message builder used in all tiers.
        let CryptographicProcessorSettings {
            max_num_mix_layers,
            max_payload_size,
            ..
        } = mix_config.message_blend.cryptographic_processor;
        let message_builder = MessageBuilder::new(max_num_mix_layers, max_payload_size).unwrap();

        // Spawn Persistent Transmission
        let (transmission_schedule_sender, transmission_schedule_receiver) =
            mpsc::unbounded_channel();
        let (emission_sender, mut emission_receiver) = mpsc::unbounded_channel();
        let drop_message = message_builder.drop_message();
        tokio::spawn(async move {
            persistent_transmission(
                mix_config.persistent_transmission,
                drop_message,
                transmission_schedule_receiver,
                emission_sender,
            )
            .await;
        });

        // Spawn Message Blend and connect it to Persistent Transmission
        let (new_message_sender, new_message_receiver) = mpsc::unbounded_channel();
        let (processor_inbound_sender, processor_inbound_receiver) = mpsc::unbounded_channel();
        let (fully_unwrapped_message_sender, mut fully_unwrapped_message_receiver) =
            mpsc::unbounded_channel();
        tokio::spawn(async move {
            MessageBlend::new(
                mix_config.message_blend,
                message_builder,
                membership.clone(),
                new_message_receiver,
                processor_inbound_receiver,
                // Connect the outputs of Message Blend to Persistent Transmission
                transmission_schedule_sender,
                fully_unwrapped_message_sender,
            )
            .run()
            .await;
        });

        // A channel to listen to messages received from the [`MixBackend`]
        let mut incoming_message_stream = backend.listen_to_incoming_messages();

        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();
        loop {
            tokio::select! {
                Some(msg) = incoming_message_stream.next() => {
                    tracing::debug!("Received message from mix backend. Sending it to Processor");
                    if let Err(e) = processor_inbound_sender.send(msg) {
                        tracing::error!("Failed to send incoming message to processor: {e:?}");
                    }
                }
                Some(msg) = emission_receiver.recv() => {
                    tracing::debug!("Emitting message to mix network");
                    backend.publish(msg).await;
                }
                Some(msg) = fully_unwrapped_message_receiver.recv() => {
                    tracing::debug!("Broadcasting fully unwrapped message");
                    match wire::deserialize::<NetworkMessage<Network::BroadcastSettings>>(&msg) {
                        Ok(msg) => {
                            network_adapter.broadcast(msg.message, msg.broadcast_settings).await;
                        },
                        _ => tracing::error!("unrecognized message from mix backend")
                    }
                }
                Some(msg) = service_state.inbound_relay.recv() => {
                    Self::handle_service_message(msg, &new_message_sender);
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
    fn handle_service_message(
        msg: ServiceMessage<Network::BroadcastSettings>,
        new_message_sender: &mpsc::UnboundedSender<Vec<u8>>,
    ) {
        match msg {
            ServiceMessage::Mix(msg) => {
                // Serialize the new message and send it to the Processor
                if let Err(e) = new_message_sender.send(wire::serialize(&msg).unwrap()) {
                    tracing::error!("Failed to send a new message to processor: {e:?}");
                }
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
    pub persistent_transmission: PersistentTransmissionSettings,
    pub message_blend: MessageBlendSettings,
    pub membership: Vec<Node>,
}

impl<BackendSettings> MixConfig<BackendSettings> {
    // TODO: This step can be redesigned once we can load membership info from the chain state.
    fn membership(&self) -> Membership {
        let local_public_key = x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(
            self.message_blend.cryptographic_processor.private_key,
        ))
        .to_bytes();
        let remote_nodes = self
            .membership
            .iter()
            .filter(|node| node.public_key != local_public_key)
            .cloned()
            .collect();
        Membership::new(remote_nodes)
    }
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
