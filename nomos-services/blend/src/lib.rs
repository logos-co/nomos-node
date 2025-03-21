pub mod backends;
pub mod network;

use std::{
    fmt::{Debug, Display},
    hash::Hash,
    time::Duration,
};

use async_trait::async_trait;
use backends::BlendBackend;
use futures::StreamExt;
use network::NetworkAdapter;
use nomos_blend::{
    cover_traffic::{CoverTraffic, CoverTrafficSettings},
    membership::{Membership, Node},
    message_blend::{
        crypto::CryptographicProcessor, temporal::TemporalScheduler,
        CryptographicProcessorSettings, MessageBlendExt, MessageBlendSettings,
    },
    persistent_transmission::{
        PersistentTransmissionExt, PersistentTransmissionSettings, PersistentTransmissionStream,
    },
    BlendOutgoingMessage,
};
use nomos_blend_message::{sphinx::SphinxMessage, BlendMessage};
use nomos_core::wire;
use nomos_network::NetworkService;
use overwatch::{
    services::{
        state::{NoOperator, NoState},
        AsServiceId, ServiceCore, ServiceData,
    },
    OpaqueServiceStateHandle,
};
use rand::SeedableRng;
use rand_chacha::ChaCha12Rng;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use services_utils::overwatch::lifecycle;
use tokio::{sync::mpsc, time};
use tokio_stream::wrappers::{IntervalStream, UnboundedReceiverStream};

/// A blend service that sends messages to the blend network
/// and broadcasts fully unwrapped messages through the [`NetworkService`].
///
/// The blend backend and the network adapter are generic types that are
/// independent with each other. For example, the blend backend can use the
/// libp2p network stack, while the network adapter can use the other network
/// backend.
pub struct BlendService<Backend, Network, RuntimeServiceId>
where
    Backend: BlendBackend + 'static,
    Backend::Settings: Clone + Debug,
    Network: NetworkAdapter<RuntimeServiceId>,
    Network::BroadcastSettings: Clone + Debug + Serialize + DeserializeOwned,
{
    backend: Backend,
    service_state: OpaqueServiceStateHandle<Self, RuntimeServiceId>,
    membership: Membership<Backend::NodeId, SphinxMessage>,
}

impl<Backend, Network, RuntimeServiceId> ServiceData
    for BlendService<Backend, Network, RuntimeServiceId>
where
    Backend: BlendBackend + 'static,
    Backend::Settings: Clone,
    Network: NetworkAdapter<RuntimeServiceId>,
    Network::BroadcastSettings: Clone + Debug + Serialize + DeserializeOwned,
{
    type Settings = BlendConfig<Backend::Settings, Backend::NodeId>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State, Self::Settings>;
    type Message = ServiceMessage<Network::BroadcastSettings>;
}

#[async_trait]
impl<Backend, Network, RuntimeServiceId> ServiceCore<RuntimeServiceId>
    for BlendService<Backend, Network, RuntimeServiceId>
where
    Backend: BlendBackend + Send + 'static,
    Backend::Settings: Clone,
    Backend::NodeId: Hash + Eq + Unpin,
    Network: NetworkAdapter<RuntimeServiceId> + Send + Sync + 'static,
    Network::BroadcastSettings:
        Clone + Debug + Serialize + DeserializeOwned + Send + Sync + 'static,
    RuntimeServiceId: AsServiceId<NetworkService<Network::Backend, RuntimeServiceId>>
        + AsServiceId<Self>
        + Clone
        + Debug
        + Display
        + Sync
        + Send,
{
    fn init(
        service_state: OpaqueServiceStateHandle<Self, RuntimeServiceId>,
        _init_state: Self::State,
    ) -> Result<Self, overwatch::DynError> {
        let blend_config = service_state.settings_reader.get_updated_settings();
        Ok(Self {
            backend: <Backend as BlendBackend>::new(
                service_state.settings_reader.get_updated_settings().backend,
                service_state.overwatch_handle.clone(),
                blend_config.membership(),
                ChaCha12Rng::from_entropy(),
            ),
            service_state,
            membership: blend_config.membership(),
        })
    }

    async fn run(mut self) -> Result<(), overwatch::DynError> {
        let Self {
            service_state,
            mut backend,
            membership,
        } = self;
        let blend_config = service_state.settings_reader.get_updated_settings();
        let mut cryptographic_processor = CryptographicProcessor::new(
            blend_config.message_blend.cryptographic_processor.clone(),
            membership.clone(),
            ChaCha12Rng::from_entropy(),
        );
        let network_relay = service_state
            .overwatch_handle
            .relay::<NetworkService<Network::Backend, RuntimeServiceId>>()
            .await?;
        let network_adapter = Network::new(network_relay);

        // tier 1 persistent transmission
        let (persistent_sender, persistent_receiver) = mpsc::unbounded_channel();
        let mut persistent_transmission_messages: PersistentTransmissionStream<_, _, _> =
            UnboundedReceiverStream::new(persistent_receiver).persistent_transmission(
                blend_config.persistent_transmission,
                ChaCha12Rng::from_entropy(),
                IntervalStream::new(time::interval(Duration::from_secs_f64(
                    1.0 / blend_config.persistent_transmission.max_emission_frequency,
                )))
                .map(|_| ()),
                SphinxMessage::DROP_MESSAGE.to_vec(),
            );

        // tier 2 blend
        let temporal_scheduler = TemporalScheduler::new(
            blend_config.message_blend.temporal_processor,
            ChaCha12Rng::from_entropy(),
        );
        let mut blend_messages = backend.listen_to_incoming_messages().blend(
            blend_config.message_blend.clone(),
            membership.clone(),
            temporal_scheduler,
            ChaCha12Rng::from_entropy(),
        );

        // tier 3 cover traffic
        let mut cover_traffic: CoverTraffic<_, _, SphinxMessage> = CoverTraffic::new(
            blend_config.cover_traffic.cover_traffic_settings(
                &membership,
                &blend_config.message_blend.cryptographic_processor,
            ),
            blend_config.cover_traffic.epoch_stream(),
            blend_config.cover_traffic.slot_stream(),
        );

        // local messages, are bypassed and send immediately
        let mut local_messages =
            service_state
                .inbound_relay
                .map(|ServiceMessage::Blend(message)| {
                    wire::serialize(&message)
                        .expect("Message from internal services should not fail to serialize")
                });

        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();
        #[expect(
            clippy::redundant_pub_crate,
            reason = "Generated by `tokio::select` macro."
        )]
        loop {
            tokio::select! {
                Some(msg) = persistent_transmission_messages.next() => {
                    backend.publish(msg).await;
                }
                // Already processed blend messages
                Some(msg) = blend_messages.next() => {
                    match msg {
                        BlendOutgoingMessage::Outbound(msg) => {
                            if let Err(e) = persistent_sender.send(msg) {
                                tracing::error!("Error sending message to persistent stream: {e}");
                            }
                        }
                        BlendOutgoingMessage::FullyUnwrapped(msg) => {
                            tracing::debug!("Broadcasting fully unwrapped message");
                            match wire::deserialize::<NetworkMessage<Network::BroadcastSettings>>(&msg) {
                                Ok(msg) => {
                                    network_adapter.broadcast(msg.message, msg.broadcast_settings).await;
                                },
                                _ => {
                                    tracing::debug!("unrecognized message from blend backend");
                                }
                            }
                        }
                    }
                }
                Some(msg) = cover_traffic.next() => {
                    Self::wrap_and_send_to_persistent_transmission(&msg, &mut cryptographic_processor, &persistent_sender);
                }
                Some(msg) = local_messages.next() => {
                    Self::wrap_and_send_to_persistent_transmission(&msg, &mut cryptographic_processor, &persistent_sender);
                }
                Some(msg) = lifecycle_stream.next() => {
                    if lifecycle::should_stop_service::<Self, RuntimeServiceId>(&msg) {
                        // TODO: Maybe add a call to backend to handle this. Maybe trying to save unprocessed messages?
                        break;
                    }
                }
            }
        }

        Ok(())
    }
}

impl<Backend, Network, RuntimeServiceId> BlendService<Backend, Network, RuntimeServiceId>
where
    Backend: BlendBackend + Send + 'static,
    Backend::Settings: Clone,
    Backend::NodeId: Hash + Eq,
    Network: NetworkAdapter<RuntimeServiceId>,
    Network::BroadcastSettings: Clone + Debug + Serialize + DeserializeOwned,
{
    fn wrap_and_send_to_persistent_transmission(
        message: &[u8],
        cryptographic_processor: &mut CryptographicProcessor<
            Backend::NodeId,
            ChaCha12Rng,
            SphinxMessage,
        >,
        persistent_sender: &mpsc::UnboundedSender<Vec<u8>>,
    ) {
        match cryptographic_processor.wrap_message(message) {
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
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BlendConfig<BackendSettings, BackendNodeId> {
    pub backend: BackendSettings,
    pub message_blend: MessageBlendSettings<SphinxMessage>,
    pub persistent_transmission: PersistentTransmissionSettings,
    pub cover_traffic: CoverTrafficExtSettings,
    pub membership:
        Vec<Node<BackendNodeId, <SphinxMessage as nomos_blend_message::BlendMessage>::PublicKey>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CoverTrafficExtSettings {
    pub epoch_duration: Duration,
    pub slot_duration: Duration,
}

impl CoverTrafficExtSettings {
    fn cover_traffic_settings<NodeId>(
        &self,
        membership: &Membership<NodeId, SphinxMessage>,
        cryptographic_processor_settings: &CryptographicProcessorSettings<
            <SphinxMessage as BlendMessage>::PrivateKey,
        >,
    ) -> CoverTrafficSettings
    where
        NodeId: Hash + Eq,
    {
        CoverTrafficSettings {
            node_id: membership.local_node().public_key,
            number_of_hops: cryptographic_processor_settings.num_blend_layers,
            slots_per_epoch: self.slots_per_epoch(),
            network_size: membership.size(),
        }
    }

    const fn slots_per_epoch(&self) -> usize {
        (self.epoch_duration.as_secs() as usize)
            .checked_div(self.slot_duration.as_secs() as usize)
            .expect("Invalid epoch & slot duration")
    }

    fn epoch_stream(
        &self,
    ) -> futures::stream::Map<
        futures::stream::Enumerate<IntervalStream>,
        impl FnMut((usize, time::Instant)) -> usize,
    > {
        IntervalStream::new(time::interval(self.epoch_duration))
            .enumerate()
            .map(|(i, _)| i)
    }

    fn slot_stream(
        &self,
    ) -> futures::stream::Map<
        futures::stream::Enumerate<IntervalStream>,
        impl FnMut((usize, time::Instant)) -> usize,
    > {
        let slots_per_epoch = self.slots_per_epoch();
        IntervalStream::new(time::interval(self.slot_duration))
            .enumerate()
            .map(move |(i, _)| i % slots_per_epoch)
    }
}

impl<BackendSettings, BackendNodeId> BlendConfig<BackendSettings, BackendNodeId>
where
    BackendNodeId: Clone + Hash + Eq,
{
    fn membership(&self) -> Membership<BackendNodeId, SphinxMessage> {
        let public_key = x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(
            self.message_blend.cryptographic_processor.private_key,
        ))
        .to_bytes();
        Membership::new(self.membership.clone(), &public_key)
    }
}

/// A message that is handled by [`BlendService`].
#[derive(Debug)]
pub enum ServiceMessage<BroadcastSettings> {
    /// To send a message to the blend network and eventually broadcast it to
    /// the [`NetworkService`].
    Blend(NetworkMessage<BroadcastSettings>),
}

/// A message that is sent to the blend network.
///
/// To eventually broadcast the message to the network service,
/// [`BroadcastSettings`] must be included in the [`NetworkMessage`].
/// [`BroadcastSettings`] is a generic type defined by [`NetworkAdapter`].
#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkMessage<BroadcastSettings> {
    pub message: Vec<u8>,
    pub broadcast_settings: BroadcastSettings,
}
