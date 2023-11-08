// std
use std::fmt::Debug;

// crates
use futures::StreamExt;
use overwatch_rs::services::ServiceError;
use tracing::error;

// internal
use overwatch_rs::services::life_cycle::LifecycleMessage;
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::RelayMessage,
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};

pub mod backend;
pub mod frontend;
pub mod types;

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
    service_state: ServiceStateHandle<Self>,
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
        backend: &Backend,
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

    async fn handle_message(message: MetricsMessage<Backend::MetricsData>, backend: &mut Backend) {
        match message {
            MetricsMessage::Load {
                service_id,
                reply_channel,
            } => {
                MetricsService::handle_load(backend, &service_id, reply_channel).await;
            }
            MetricsMessage::Update { service_id, data } => {
                MetricsService::handle_update(backend, &service_id, data).await;
            }
        }
    }

    async fn should_stop_service(message: LifecycleMessage) -> bool {
        match message {
            LifecycleMessage::Shutdown(sender) => {
                if sender.send(()).is_err() {
                    error!(
                        "Error sending successful shutdown signal from service {}",
                        Self::SERVICE_ID
                    );
                }
                true
            }
            LifecycleMessage::Kill => true,
        }
    }
}

#[async_trait::async_trait]
impl<Backend: MetricsBackend + Send + Sync + 'static> ServiceCore for MetricsService<Backend> {
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, ServiceError> {
        let settings = service_state.settings_reader.get_updated_settings();
        let backend = Backend::init(settings);
        Ok(Self {
            service_state,
            backend,
        })
    }

    async fn run(self) -> Result<(), ServiceError> {
        let Self {
            service_state:
                ServiceStateHandle {
                    mut inbound_relay,
                    lifecycle_handle,
                    ..
                },
            mut backend,
        } = self;
        let mut lifecycle_stream = lifecycle_handle.message_stream();
        loop {
            tokio::select! {
                Some(message) = inbound_relay.recv() => {
                    Self::handle_message(message, &mut backend).await;
                }
                Some(message) = lifecycle_stream.next() => {
                    if Self::should_stop_service(message).await {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}
