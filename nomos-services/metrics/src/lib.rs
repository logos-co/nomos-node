use std::{
    borrow::Cow,
    collections::HashMap,
    sync::Arc,
    time::Duration,
};

use axum::{Extension, Server, Router, http::{HeaderValue, Method, header::{CONTENT_TYPE, USER_AGENT}}};
use futures::future::BoxFuture;
use overwatch_rs::{services::{
    handle::ServiceStateHandle,
    relay::{RelayMessage, InboundRelay},
    state::{NoOperator, NoState},
    ServiceCore, ServiceData,
}, overwatch::{handle::OverwatchHandle, commands::{OverwatchCommand, RelayCommand}}};
use parking_lot::Mutex;
use serde::{Serialize, Deserialize};
use tower_http::{cors::{CorsLayer, Any}, trace::TraceLayer};

pub struct Metrics<E> 
    where E: core::fmt::Debug + Send + Sync + 'static
{
    handle: OverwatchHandle,
    settings: MetricsSettings,
    inbound_relay: InboundRelay<MetricsMessage<E>>,
}

#[derive(Debug, Clone)]
struct MetricsBackend<E> {
    stack: Arc<Mutex<HashMap<Cow<'static, str>, Vec<Data<E>>>>>,
}

impl<E> Default for MetricsBackend<E> {
    fn default() -> Self {
        Self {
            stack: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_graphql::Object]
impl<E> MetricsBackend<E> 
where E: core::fmt::Debug + Clone + Send + Sync + 'static + async_graphql::OutputType
{
    pub async fn load(&self, name: String) -> Option<Vec<Data<E>>> {
        let map = self.stack.lock();
        map.get(name.as_str()).cloned()
    }
}


#[derive(Debug, Clone, clap::Parser)]
pub struct MetricsSettings
{
    #[command(flatten)]
    graphql: GraphqlServerSettings,
}


/// Configuration for the GraphQl Server
#[derive(Debug, Clone, clap::Args)]
pub(crate) struct GraphqlServerSettings {
    /// Socket where the GraphQL will be listening on for incoming requests.
    #[arg(short, long = "graphql-addr", env = "METRICS_GRAPHQL_BIND_ADDRESS")]
    pub address: std::net::SocketAddr,
    /// Max query depth allowed
    #[arg(long = "graphql-max-depth", default_value_t = 20, env = "METRICS_GRAPHQL_MAX_DEPTH")]
    pub max_depth: usize,
    /// Max query complexity allowed
    #[arg(
        long = "graphql-max-complexity",
        default_value_t = 1000,
        env = "METRICS_GRAPHQL_MAX_COMPLEXITY"
    )]
    pub max_complexity: usize,
    /// Allowed origins for this server deployment requests.
    #[arg(long = "graphql-cors-origin")]
    pub cors_origins: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricsMessage<E> {
    Load {
        service_id: Cow<'static, str>,
    },
    Update {
        service_id: Cow<'static, str>,
        msg: Data<E>
    }
}

impl<E> RelayMessage for MetricsMessage<E> 
    where E: 'static {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Data<E> {
    duration: Duration,
    extensions: E,
}

#[async_graphql::Object]
impl<E> Data<E> 
    where E: async_graphql::OutputType + Send + Sync + 'static
{
    pub async fn duration(&self) -> u64 {
        self.duration.as_millis() as u64
    }

    pub async fn extensions(&self) -> &E {
        &self.extensions
    }
}


impl<E> ServiceData for Metrics<E> 
    where E: core::fmt::Debug + Send + Sync + 'static
{
    const SERVICE_ID: &'static str = "Metrics";
    type Settings = MetricsSettings;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = MetricsMessage<E>;
}


#[async_trait::async_trait]
impl<E> ServiceCore for Metrics<E>
    where E:  core::fmt::Debug + Send + Sync + 'static,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Self {
        let ServiceStateHandle {
            inbound_relay,
            overwatch_handle,
            mut settings_reader,
            state_updater: _,
            _lifecycle_handler: _,
        } = service_state;
        
        Self {
            handle: overwatch_handle,
            inbound_relay,
            settings: settings_reader.get_updated_settings(),
        }
    }

    async fn run(self) {
        let Self {
            mut handle,
            settings,
            mut inbound_relay,
        } = self;

        let backend = MetricsBackend::<E>::default();

        // thread for handling update metrics
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    msg = inbound_relay.recv() => {
                        if let Some(msg) = msg {
                            match msg {
                                MetricsMessage::Load { service_id } => {
                                    // TODO: relay to the caller
                                    todo!()
                                },
                                MetricsMessage::Update { service_id, msg } => {
                                    let mut map = backend.stack.lock();
                                    match map.entry(service_id) {
                                        std::collections::hash_map::Entry::Occupied(mut val) => val.get_mut().push(msg),
                                        std::collections::hash_map::Entry::Vacant(val) => {
                                            val.insert(vec![msg]);
                                        },
                                    }
                                },
                            }
                        }
                    }
                }
            }
        });

        // thread for handling graphql query
        tokio::spawn(async move {
            

            let mut builder = CorsLayer::new();
            if settings.graphql.cors_origins.is_empty() {
                builder = builder.allow_origin(Any);
            }
            for origin in settings.graphql.cors_origins {
                builder = builder.allow_origin(origin.as_str().parse::<HeaderValue>().expect("fail to parse origin"));
            }
            let cors = builder
                .allow_headers([CONTENT_TYPE, USER_AGENT])
                .allow_methods(Any);

            let router = Router::new()
                .route("/*path", get(index).post(graphql_handler::<P>))
                .layer(Extension(schema))
                .layer(cors)
                .layer(TraceLayer::new_for_http());

            tracing::info!("Metrics Service GraphQL server listening: {}", settings.graphql.address);
            Server::bind(&settings.graphql.address)
                .serve(router.into_make_service())
                .await
                .unwrap();
        });

    }
}