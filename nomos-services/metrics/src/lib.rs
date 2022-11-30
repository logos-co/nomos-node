pub mod backend;
pub mod frontend;

use std::error::Error;
use std::fmt::Debug;
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
    type Error: Error + Send + Sync;
    type Settings: Clone + Send + Sync + 'static;
    fn init(config: Self::Settings) -> Self;
    async fn update(&mut self, data: Self::MetricsData);
    async fn load(&mut self, service_id: &ServiceId) -> Option<&Self::MetricsData>;
}

pub struct MetricsService<Backend: MetricsBackend> {
    inbound_relay: InboundRelay<MetricsMessage<Backend::MetricsData>>,
    backend: Backend,
}

#[derive(Debug)]
pub enum MetricsMessage<Data> {
    Load {
        service_id: ServiceId,
        reply_channel: tokio::sync::oneshot::Sender<Option<Data>>,
    },
    Update {
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
        service_id: &ServiceId,
        reply_channel: tokio::sync::oneshot::Sender<Option<Backend::MetricsData>>,
    ) {
        let metrics = backend.load(service_id).await.cloned();
        if let Err(e) = reply_channel.send(metrics) {
            tracing::error!(
                "Failed to send metric data for service: {service_id}. data: {:?}",
                e
            );
        }
    }

    async fn handle_update(backend: &mut Backend, metrics: Backend::MetricsData) {
        backend.update(metrics).await;
    }
}

#[async_trait::async_trait]
impl<Backend: MetricsBackend + Send + Sync + 'static> ServiceCore for MetricsService<Backend> {
    fn init(mut service_state: ServiceStateHandle<Self>) -> Self {
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
                    MetricsMessage::Update { data } => {
                        MetricsService::handle_update(&mut backend, data).await;
                    }
                }
            }
        });
    }
}
