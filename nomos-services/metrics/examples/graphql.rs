use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use clap::Parser;
use metrics::{
    frontend::graphql::{Graphql, GraphqlServerSettings},
    MetricsBackend, MetricsMessage, MetricsService, OwnedServiceId,
};
use overwatch_rs::{
    overwatch::OverwatchRunner,
    services::{
        handle::{ServiceHandle, ServiceStateHandle},
        relay::{NoMessage, Relay},
        state::{NoOperator, NoState},
        ServiceCore, ServiceData, ServiceId,
    },
};

#[derive(Debug, Clone)]
pub struct ConcurrentMapMetricsBackend(Arc<Mutex<HashMap<ServiceId, MetricsData>>>);

#[async_trait::async_trait]
impl MetricsBackend for ConcurrentMapMetricsBackend {
    type MetricsData = MetricsData;
    type Error = ();
    type Settings = &'static [ServiceId];

    fn init(config: Self::Settings) -> Self {
        let mut map = HashMap::with_capacity(config.len());
        for service_id in config {
            map.insert(*service_id, MetricsData::default());
        }
        Self(Arc::new(Mutex::new(map)))
    }

    async fn update(&mut self, service_id: ServiceId, data: Self::MetricsData) {
        self.0.lock().unwrap().insert(service_id, data);
    }

    async fn load(&self, service_id: &OwnedServiceId) -> Option<Self::MetricsData> {
        self.0.lock().unwrap().get(service_id.as_ref()).cloned()
    }
}

#[derive(Clone)]
pub struct MetricsUpdater<Backend: MetricsBackend + Send + Sync + 'static> {
    backend_channel: Relay<MetricsService<Backend>>,
}

impl<Backend: MetricsBackend + Send + Sync + 'static> ServiceData for MetricsUpdater<Backend> {
    const SERVICE_ID: ServiceId = "MetricsUpdater";

    type Settings = ();

    type State = NoState<()>;

    type StateOperator = NoOperator<Self::State>;

    type Message = NoMessage;
}

#[async_trait::async_trait]
impl<Backend: MetricsBackend<MetricsData = MetricsData> + Clone + Send + Sync + 'static> ServiceCore
    for MetricsUpdater<Backend>
where
    Backend::MetricsData: async_graphql::OutputType,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        let backend_channel: Relay<MetricsService<Backend>> =
            service_state.overwatch_handle.relay();
        Ok(Self { backend_channel })
    }

    async fn run(self) -> Result<(), overwatch_rs::DynError> {
        let mut duration = 0;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            duration += 1;
            let replay = self.backend_channel.clone().connect().await.map_err(|e| {
                tracing::error!(err = ?e, "Metrics Updater: relay connect error");
                e
            })?;

            if duration % 2 == 0 {
                replay
                    .send(MetricsMessage::Update {
                        service_id: "Foo",
                        data: MetricsData { duration },
                    })
                    .await
                    .map_err(|(e, _)| {
                        tracing::error!(err = ?e, "Metrics Updater: relay send error");
                        e
                    })?;
            } else {
                replay
                    .send(MetricsMessage::Update {
                        service_id: "Bar",
                        data: MetricsData { duration },
                    })
                    .await
                    .map_err(|(e, _)| {
                        tracing::error!(err = ?e, "Metrics Updater: send error");
                        e
                    })?;
            }
        }
    }
}

#[derive(overwatch_derive::Services)]
struct Services {
    graphql: ServiceHandle<Graphql<ConcurrentMapMetricsBackend>>,
    metrics: ServiceHandle<MetricsService<ConcurrentMapMetricsBackend>>,
    updater: ServiceHandle<MetricsUpdater<ConcurrentMapMetricsBackend>>,
}

#[derive(clap::Parser)]
pub struct Args {
    #[clap(flatten)]
    graphql: GraphqlServerSettings,
}

#[derive(Debug, Default, Clone, async_graphql::SimpleObject)]
pub struct MetricsData {
    duration: u64,
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let settings = Args::parse();
    let graphql = OverwatchRunner::<Services>::run(
        ServicesServiceSettings {
            graphql: settings.graphql,
            metrics: &["Foo", "Bar"],
            updater: (),
        },
        None,
    )?;

    tracing_subscriber::fmt::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_owned()))
        .with_file(false)
        .init();

    graphql.wait_finished();
    Ok(())
}
