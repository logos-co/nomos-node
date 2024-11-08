pub mod backends;
pub mod network;

use async_trait::async_trait;
use backends::MixBackend;
use futures::StreamExt;
use network::NetworkAdapter;
use nomos_core::wire;
use nomos_mix::membership::{Membership, Node};
use nomos_mix::message_blend::crypto::CryptographicProcessor;
use nomos_mix::message_blend::temporal::TemporalScheduler;
use nomos_mix::message_blend::{MessageBlendExt, MessageBlendSettings};
use nomos_mix::persistent_transmission::{
    PersistentTransmissionExt, PersistentTransmissionSettings, PersistentTransmissionStream,
};
use nomos_mix::MixOutgoingMessage;
use nomos_mix_message::mock::MockMixMessage;
use nomos_network::NetworkService;
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    life_cycle::LifecycleMessage,
    relay::{Relay, RelayMessage},
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};
use rand::SeedableRng;
use rand_chacha::ChaCha12Rng;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fmt::Debug;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time;
use tokio_stream::wrappers::{IntervalStream, UnboundedReceiverStream};

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
    membership: Membership<MockMixMessage>,
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
        let mix_config = service_state.settings_reader.get_updated_settings();
        Ok(Self {
            backend: <Backend as MixBackend>::new(
                service_state.settings_reader.get_updated_settings().backend,
                service_state.overwatch_handle.clone(),
                mix_config.membership(),
                ChaCha12Rng::from_entropy(),
            ),
            service_state,
            network_relay,
            membership: mix_config.membership(),
        })
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
        let Self {
            service_state,
            mut backend,
            network_relay,
            membership,
        } = self;
        let mix_config = service_state.settings_reader.get_updated_settings();
        let mut cryptographic_processor = CryptographicProcessor::new(
            mix_config.message_blend.cryptographic_processor.clone(),
            membership.clone(),
            ChaCha12Rng::from_entropy(),
        );
        let network_relay = network_relay.connect().await?;
        let network_adapter = Network::new(network_relay);

        // tier 1 persistent transmission
        let (persistent_sender, persistent_receiver) = mpsc::unbounded_channel();
        let mut persistent_transmission_messages: PersistentTransmissionStream<
            _,
            _,
            MockMixMessage,
            _,
        > = UnboundedReceiverStream::new(persistent_receiver).persistent_transmission(
            mix_config.persistent_transmission,
            ChaCha12Rng::from_entropy(),
            IntervalStream::new(time::interval(Duration::from_secs_f64(
                1.0 / mix_config.persistent_transmission.max_emission_frequency,
            )))
            .map(|_| ()),
        );

        // tier 2 blend
        let temporal_scheduler = TemporalScheduler::new(
            mix_config.message_blend.temporal_processor,
            ChaCha12Rng::from_entropy(),
        );
        let mut blend_messages = backend.listen_to_incoming_messages().blend(
            mix_config.message_blend,
            membership.clone(),
            temporal_scheduler,
            ChaCha12Rng::from_entropy(),
        );

        // local messages, are bypassed and send immediately
        let mut local_messages = service_state
            .inbound_relay
            .map(|ServiceMessage::Mix(message)| {
                wire::serialize(&message)
                    .expect("Message from internal services should not fail to serialize")
            });

        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();
        loop {
            tokio::select! {
                Some(msg) = persistent_transmission_messages.next() => {
                    backend.publish(msg).await;
                }
                // Already processed blend messages
                Some(msg) = blend_messages.next() => {
                    match msg {
                        MixOutgoingMessage::Outbound(msg) => {
                            if let Err(e) = persistent_sender.send(msg) {
                                tracing::error!("Error sending message to persistent stream: {e}");
                            }
                        }
                        MixOutgoingMessage::FullyUnwrapped(msg) => {
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
                Some(msg) = local_messages.next() => {
                    match cryptographic_processor.wrap_message(&msg) {
                        Ok(wrapped_message) => {
                            if let Err(e) = persistent_sender.send(wrapped_message) {
                                tracing::error!("Error sending message to persistent stream: {e}");
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to wrap message: {:?}", e);
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
    pub message_blend: MessageBlendSettings<MockMixMessage>,
    pub persistent_transmission: PersistentTransmissionSettings,
    pub membership: Vec<
        Node<<nomos_mix_message::mock::MockMixMessage as nomos_mix_message::MixMessage>::PublicKey>,
    >,
}

impl<BackendSettings> MixConfig<BackendSettings> {
    fn membership(&self) -> Membership<MockMixMessage> {
        // We use private key as a public key because the `MockMixMessage` doesn't differentiate between them.
        // TODO: Convert private key to public key properly once the real MixMessage is implemented.
        let public_key = self.message_blend.cryptographic_processor.private_key;
        Membership::new(self.membership.clone(), public_key)
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
