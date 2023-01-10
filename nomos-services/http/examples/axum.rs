use clap::Parser;
use nomos_http::{
    backends::{
        axum::{AxumBackend, AxumBackendSettings},
        HttpBackend,
    },
    *,
};
use overwatch_rs::{overwatch::OverwatchRunner, services::handle::ServiceHandle};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc::channel;

use metrics::{MetricsBackend, MetricsMessage, MetricsService, OwnedServiceId};
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::{NoMessage, Relay},
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};

#[derive(Debug, Clone)]
pub struct Foo;

#[derive(overwatch_derive::Services)]
struct Services {
    http: ServiceHandle<HttpService<AxumBackend>>,
    metrics: ServiceHandle<MetricsService<ConcurrentMapMetricsBackend>>,
}

#[derive(clap::Parser)]
pub struct Args {
    #[clap(flatten)]
    http: AxumBackendSettings,
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let settings = Args::parse();
    let app = OverwatchRunner::<Services>::run(
        ServicesServiceSettings {
            http: nomos_http::Config {
                backend: settings.http,
            },
            metrics: &["Foo", "Bar"],
        },
        None,
    )?;

    tracing_subscriber::fmt::fmt().with_file(false).init();

    tracing::info!("overwatch ready");
    app.wait_finished();
    Ok(())
}

// TODO: This is borrowed from metrics example. Replace with independent service that
// returns "hello world" or similar.

#[derive(Debug, Default, Clone, async_graphql::SimpleObject)]
pub struct MetricsData {
    duration: u64,
}

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
pub struct MetricsUpdater<M: MetricsBackend + Send + Sync + 'static, H: HttpBackend> {
    backend_channel: Relay<MetricsService<M>>,
    http_relay: Relay<HttpService<H>>,
}

impl<Backend: MetricsBackend + Send + Sync + 'static, H: HttpBackend> ServiceData
    for MetricsUpdater<Backend, H>
{
    const SERVICE_ID: ServiceId = "MetricsUpdater";

    type Settings = ();

    type State = NoState<()>;

    type StateOperator = NoOperator<Self::State>;

    type Message = NoMessage;
}

#[async_trait::async_trait]
impl<Backend, H> ServiceCore for MetricsUpdater<Backend, H>
where
    Backend: MetricsBackend<MetricsData = MetricsData> + Clone + Send + Sync + 'static,
    Backend::MetricsData: async_graphql::OutputType,
    H: HttpBackend<Response = String>,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        let backend_channel: Relay<MetricsService<Backend>> =
            service_state.overwatch_handle.relay();
        let http_relay: Relay<HttpService<H>> = service_state.overwatch_handle.relay();
        Ok(Self {
            backend_channel,
            http_relay,
        })
    }

    async fn run(self) -> Result<(), overwatch_rs::DynError> {
        let replay = self.backend_channel.connect().await.map_err(|e| {
            tracing::error!(err = ?e, "Metrics Updater: relay connect error");
            e
        })?;

        let http = self.http_relay.connect().await.map_err(|e| {
            tracing::error!(err = ?e, "Metrics Updater: http relay connect error");
            e
        })?;

        let (req_stream, mut req_rx) = channel(1);

        _ = http
            .send(HttpMsg::AddHandler {
                service_id: "metrics",
                route: Route {
                    method: HttpMethod::GET,
                    // If empty string is passed, the route will be "http://{addr}/metrics/"
                    path: "".into(),
                },
                req_stream,
            })
            .await;

        // Handle the http request to metrics service.
        tokio::spawn(async move {
            while let Some(req) = req_rx.recv().await {
                // TODO: handle request.
                if let Err(e) = req.res_tx.send("metrics".to_string()).await {
                    tracing::error!("metrics updater send error: {e}");
                }
            }
        });

        let tags = ["foo", "bar", "baz"].iter().cycle();
        for (duration, service_id) in tags.enumerate() {
            let message = MetricsMessage::Update {
                service_id,
                data: MetricsData {
                    duration: duration as u64,
                },
            };
            replay.send(message).await.map_err(|(e, _)| {
                tracing::error!(err = ?e, "Metrics Updater: relay send error");
                e
            })?;
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        Ok(())
    }
}
