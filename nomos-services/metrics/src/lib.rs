use std::fmt::Debug;

pub mod backend;
pub mod frontend;

// use metrics_derive::MetricsData;
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::{InboundRelay, RelayMessage},
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};

#[async_trait::async_trait]
pub trait MetricsBackend {
    type MetricsData: Clone + Send + Sync + Debug + 'static;
    type Error: Send + Sync;
    type Settings: Clone + Send + Sync + 'static;
    fn init(config: Self::Settings) -> Self;
    async fn update(&mut self, service_id: ServiceId, data: Self::MetricsData);
    async fn load(&self, service_id: &OwnedServiceId) -> Option<Self::MetricsData>;
}

pub struct MetricsService<Backend: MetricsBackend> {
    inbound_relay: InboundRelay<MetricsMessage<Backend::MetricsData>>,
    backend: Backend,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct OwnedServiceId {
    id: String,
}

impl From<ServiceId> for OwnedServiceId {
    fn from(id: ServiceId) -> Self {
        Self { id: id.into() }
    }
}

impl From<String> for OwnedServiceId {
    fn from(id: String) -> Self {
        Self { id }
    }
}

impl core::fmt::Display for OwnedServiceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl AsRef<str> for OwnedServiceId {
    fn as_ref(&self) -> &str {
        &self.id
    }
}

#[cfg(feature = "async-graphql")]
impl async_graphql::InputType for OwnedServiceId {
    type RawValueType = Self;

    fn type_name() -> std::borrow::Cow<'static, str> {
        <String as async_graphql::InputType>::type_name()
    }

    fn create_type_info(registry: &mut async_graphql::registry::Registry) -> String {
        <String as async_graphql::InputType>::create_type_info(registry)
    }

    fn parse(
        value: Option<async_graphql::Value>,
    ) -> async_graphql::InputValueResult<OwnedServiceId> {
        <String as async_graphql::InputType>::parse(value)
            .map(Self::from)
            .map_err(async_graphql::InputValueError::propagate)
    }

    fn to_value(&self) -> async_graphql::Value {
        <String as async_graphql::InputType>::to_value(&self.id)
    }

    fn as_raw_value(&self) -> Option<&Self::RawValueType> {
        Some(self)
    }
}

#[derive(Debug)]
pub enum MetricsMessage<Data> {
    Load {
        service_id: OwnedServiceId,
        reply_channel: tokio::sync::oneshot::Sender<Option<Data>>,
    },
    Update {
        service_id: ServiceId,
        data: Data,
    },
}

impl<Data: Send + Sync + 'static> RelayMessage for MetricsMessage<Data> {}

impl<Backend: MetricsBackend> ServiceData for MetricsService<Backend> {
    const SERVICE_ID: ServiceId = "Metrics";
    type Settings = Backend::Settings;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = MetricsMessage<Backend::MetricsData>;
}

impl<Backend: MetricsBackend> MetricsService<Backend> {
    async fn handle_load(
        backend: &mut Backend,
        service_id: &OwnedServiceId,
        reply_channel: tokio::sync::oneshot::Sender<Option<Backend::MetricsData>>,
    ) {
        let metrics = backend.load(service_id).await;
        if let Err(e) = reply_channel.send(metrics) {
            tracing::error!(
                "Failed to send metric data for service: {service_id}. data: {:?}",
                e
            );
        }
    }

    async fn handle_update(
        backend: &mut Backend,
        service_id: &ServiceId,
        metrics: Backend::MetricsData,
    ) {
        backend.update(service_id, metrics).await;
    }
}

#[async_trait::async_trait]
impl<Backend: MetricsBackend + Send + Sync + 'static> ServiceCore for MetricsService<Backend> {
    fn init(service_state: ServiceStateHandle<Self>) -> Self {
        let settings = service_state.settings_reader.get_updated_settings();
        let backend = Backend::init(settings);
        let inbound_relay = service_state.inbound_relay;
        Self {
            inbound_relay,
            backend,
        }
    }

    async fn run(self) {
        let Self {
            mut inbound_relay,
            mut backend,
        } = self;
        // thread for handling update metrics
        tokio::spawn(async move {
            while let Some(message) = inbound_relay.recv().await {
                match message {
                    MetricsMessage::Load {
                        service_id,
                        reply_channel,
                    } => {
                        MetricsService::handle_load(&mut backend, &service_id, reply_channel).await;
                    }
                    MetricsMessage::Update { service_id, data } => {
                        MetricsService::handle_update(&mut backend, &service_id, data).await;
                    }
                }
            }
        });
    }
}
