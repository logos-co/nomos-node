use std::{collections::HashMap, sync::{Arc, Mutex}};

use clap::Parser;
use metrics::{
    frontend::graphql::{Graphql, GraphqlServerSettings},
    MetricsService, OwnedServiceId, MetricsBackend,
};
use overwatch_rs::{overwatch::OverwatchRunner, services::{handle::ServiceHandle, ServiceId}};

#[derive(Debug, Clone)]
pub struct ConcurrentMapMetricsBackend(Arc<Mutex<HashMap<ServiceId, MetricsData>>>);

#[async_trait::async_trait]
impl MetricsBackend for ConcurrentMapMetricsBackend
{
    type MetricsData = MetricsData;
    type Error = ();
    type Settings = &'static [ServiceId];

    fn init(config: Self::Settings) -> Self {
        let mut map = HashMap::with_capacity(config.len());
        for svc in config.iter() {
            map.insert(*svc, MetricsData::default());
        }
        let map = Arc::new(Mutex::new(map));
        let t_map = map.clone();
        tokio::spawn(async move {
            let mut duration = 0;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                let mut map = map.lock().unwrap();
                for src in config.iter() {
                    map.insert(*src, MetricsData {
                        duration,
                    });
                }
                duration += 1;
            }
        });
        Self(t_map)
    }

    async fn update(&mut self, service_id: ServiceId, data: Self::MetricsData) {
        self.0.lock().unwrap().insert(service_id, data);
    }

    async fn load(&self, service_id: &OwnedServiceId) -> Option<Self::MetricsData> {
        self.0.lock().unwrap().get(service_id.as_ref()).cloned()
    }
}

#[derive(overwatch_derive::Services)]
struct Services {
    graphql: ServiceHandle<Graphql<ConcurrentMapMetricsBackend>>,
    metrics: ServiceHandle<MetricsService<ConcurrentMapMetricsBackend>>,
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
            metrics: &["Foo", "Network"],
        },
        None,
    );

    tracing_subscriber::fmt::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_owned()))
        .with_file(false)
        .init();

    graphql.wait_finished();
    Ok(())
}
