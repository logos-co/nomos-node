use metrics::{MetricsService, frontend::graphql::{GraphqlServerSettings, Graphql}, backend::map::MapMetricsBackend};
use clap::Parser;
use overwatch_rs::{overwatch::OverwatchRunner, services::handle::ServiceHandle};

#[derive(overwatch_derive::Services)]
struct Services {
    graphql: ServiceHandle<Graphql<MapMetricsBackend<MetricsData>>>,
    metrics: ServiceHandle<MetricsService<MapMetricsBackend<MetricsData>>>,
}

#[derive(clap::Parser)]
pub struct Args {
    #[clap(flatten)]
    graphql: GraphqlServerSettings,
}

#[derive(Debug, Clone, async_graphql::SimpleObject)]
pub struct MetricsData {
    duration: u64,
}



fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let settings = Args::parse();
    let graphql = OverwatchRunner::<Services>::run(
        ServicesServiceSettings {
            graphql: settings.graphql,
            metrics: (),
        },
        None
    );
    
    tracing_subscriber::fmt::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_owned()))
        .with_file(false)
        .init();

    graphql.wait_finished();
    Ok(())
}