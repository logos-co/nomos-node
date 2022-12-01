// std

// crates
use async_graphql::{EmptyMutation, EmptySubscription, Schema};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    extract::Path,
    http::{
        header::{CONTENT_TYPE, USER_AGENT},
        HeaderValue,
    },
    routing::post,
    Extension, Router, Server,
};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

// internal
use crate::{MetricsBackend, MetricsMessage, MetricsService, OwnedServiceId};
use overwatch_rs::services::relay::{OutboundRelay, Relay};
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::NoMessage,
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};

/// Configuration for the GraphQl Server
#[derive(Debug, Clone, clap::Args)]
pub struct GraphqlServerSettings {
    /// Socket where the GraphQL will be listening on for incoming requests.
    #[arg(short, long = "graphql-addr", env = "METRICS_GRAPHQL_BIND_ADDRESS")]
    pub address: std::net::SocketAddr,
    /// Max query depth allowed
    #[arg(
        long = "graphql-max-depth",
        default_value_t = 20,
        env = "METRICS_GRAPHQL_MAX_DEPTH"
    )]
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

async fn graphql_handler<Backend: MetricsBackend + Send + 'static + Sync>(
    Path(path): Path<String>,
    schema: Extension<Schema<Graphql<Backend>, EmptyMutation, EmptySubscription>>,
    req: GraphQLRequest,
) -> GraphQLResponse
where
    Backend::MetricsData: async_graphql::OutputType,
{
    drop(path);
    let request = req.into_inner();
    let resp = schema.execute(request).await;
    GraphQLResponse::from(resp)
}

#[derive(Clone)]
pub struct Graphql<Backend: MetricsBackend + Send + Sync + 'static> {
    settings: GraphqlServerSettings,
    backend_channel: Relay<MetricsService<Backend>>,
}

impl<Backend: MetricsBackend + Send + Sync + 'static> ServiceData for Graphql<Backend> {
    const SERVICE_ID: ServiceId = "GraphqlMetrics";

    type Settings = GraphqlServerSettings;

    type State = NoState<GraphqlServerSettings>;

    type StateOperator = NoOperator<Self::State>;

    type Message = NoMessage;
}

#[async_graphql::Object]
impl<Backend: MetricsBackend + Send + Sync + 'static> Graphql<Backend>
where
    Backend::MetricsData: async_graphql::OutputType,
{
    pub async fn load(
        &self,
        service_id: OwnedServiceId,
    ) -> async_graphql::Result<Option<<Backend as MetricsBackend>::MetricsData>> {
        // let (tx, rx) = tokio::sync::oneshot::channel();
        // self.backend_channel.send(MetricsMessage::Load {
        //     service_id: service_id.into(),
        //     reply_channel: tx,
        // }).await.map_err(|(e, _)| async_graphql::Error::new(e))?;
        // rx.await.map_err(|e| {
        //     tracing::error!(err = ?e, "GraphqlMetrics: recv error");
        //     async_graphql::Error::new("GraphqlMetrics: recv error")
        // })
        Ok(None)
    }
}

#[async_trait::async_trait]
impl<Backend: MetricsBackend + Clone + Send + Sync + 'static> ServiceCore for Graphql<Backend>
where
    Backend::MetricsData: async_graphql::OutputType,
{
    fn init(mut service_state: ServiceStateHandle<Self>) -> Self {
        let settings = service_state.settings_reader.get_updated_settings();
        let backend_channel: Relay<MetricsService<Backend>> =
            service_state.overwatch_handle.relay();
        Self {
            settings,
            backend_channel,
        }
    }

    async fn run(self) {
        // thread for handling graphql query
        tokio::spawn(async move {
            let mut builder = CorsLayer::new();
            if self.settings.cors_origins.is_empty() {
                builder = builder.allow_origin(Any);
            }
            for origin in &self.settings.cors_origins {
                builder = builder.allow_origin(
                    origin
                        .as_str()
                        .parse::<HeaderValue>()
                        .expect("fail to parse origin"),
                );
            }
            let cors = builder
                .allow_headers([CONTENT_TYPE, USER_AGENT])
                .allow_methods(Any);

            let addr = self.settings.address;
            let router = Router::new()
                .route("/*path", post(graphql_handler::<Backend>))
                .layer(Extension(self))
                .layer(cors)
                .layer(TraceLayer::new_for_http());

            tracing::info!("Metrics Service GraphQL server listening: {}", addr);
            Server::bind(&addr)
                .serve(router.into_make_service())
                .await
                .unwrap();
        });
    }
}
