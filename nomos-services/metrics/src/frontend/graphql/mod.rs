// std

// crates
use nomos_http::backends::HttpBackend;
use nomos_http::bridge::{build_http_bridge, HttpBridgeRunner};
use nomos_http::http::{handle_graphql_req, HttpMethod, HttpRequest};
// internal
use crate::{MetricsBackend, MetricsMessage, MetricsService, OwnedServiceId};
use overwatch_rs::services::relay::{Relay, RelayMessage};
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};

/// Configuration for the GraphQl Server
#[derive(Debug, Clone, clap::Args, serde::Deserialize, serde::Serialize)]
#[cfg(feature = "gql")]
pub struct GraphqlServerSettings {
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

pub fn metrics_graphql_router<MB, B>(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner
where
    MB: MetricsBackend + Clone + Send + 'static + Sync,
    MB::MetricsData: async_graphql::OutputType,
    B: HttpBackend + Send + Sync + 'static,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    Box::new(Box::pin(async move {
        // TODO: Graphql supports http GET requests, should nomos support that?
        let (relay, mut res_rx) =
            build_http_bridge::<Graphql<MB>, B, _>(handle, HttpMethod::POST, "")
                .await
                .unwrap();

        while let Some(HttpRequest {
            query: _,
            payload,
            res_tx,
        }) = res_rx.recv().await
        {
            let res = match handle_graphql_req(payload, relay.clone(), |req, tx| {
                Ok(GraphqlMetricsMessage {
                    req,
                    reply_channel: tx,
                })
            })
            .await
            {
                Ok(r) => r,
                Err(err) => {
                    tracing::error!(err);
                    err.to_string()
                }
            };

            res_tx.send(Ok(res.into())).await.unwrap();
        }
        Ok(())
    }))
}

#[derive(Debug)]
pub struct GraphqlMetricsMessage {
    req: async_graphql::Request,
    reply_channel: tokio::sync::oneshot::Sender<async_graphql::Response>,
}

impl RelayMessage for GraphqlMetricsMessage {}

pub struct Graphql<Backend>
where
    Backend: MetricsBackend + Send + Sync + 'static,
    Backend::MetricsData: async_graphql::OutputType,
{
    service_state: Option<ServiceStateHandle<Self>>,
    settings: GraphqlServerSettings,
    backend_channel: Relay<MetricsService<Backend>>,
}

impl<Backend> ServiceData for Graphql<Backend>
where
    Backend: MetricsBackend + Send + Sync + 'static,
    Backend::MetricsData: async_graphql::OutputType,
{
    const SERVICE_ID: ServiceId = "GraphqlMetrics";

    type Settings = GraphqlServerSettings;

    type State = NoState<GraphqlServerSettings>;

    type StateOperator = NoOperator<Self::State>;

    type Message = GraphqlMetricsMessage;
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
        let replay = self
            .backend_channel
            .clone()
            .connect()
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        replay
            .send(MetricsMessage::Load {
                service_id,
                reply_channel: tx,
            })
            .await
            .map_err(|(e, _)| async_graphql::Error::new(e.to_string()))?;
        rx.await.map_err(|e| {
            tracing::error!(err = ?e, "GraphqlMetrics: recv error");
            async_graphql::Error::new("GraphqlMetrics: recv error")
        })
    }
}

#[async_trait::async_trait]
impl<Backend: MetricsBackend + Send + Sync + 'static> ServiceCore for Graphql<Backend>
where
    Backend::MetricsData: async_graphql::OutputType,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        let backend_channel: Relay<MetricsService<Backend>> =
            service_state.overwatch_handle.relay();
        let settings = service_state.settings_reader.get_updated_settings();
        Ok(Self {
            settings,
            service_state: Some(service_state),
            backend_channel,
        })
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
        let max_complexity = self.settings.max_complexity;
        let max_depth = self.settings.max_depth;
        let mut inbound_relay = self.service_state.take().unwrap().inbound_relay;
        let schema = async_graphql::Schema::build(
            self,
            async_graphql::EmptyMutation,
            async_graphql::EmptySubscription,
        )
        .limit_complexity(max_complexity)
        .limit_depth(max_depth)
        .extension(async_graphql::extensions::Tracing)
        .finish();

        tracing::info!(schema = %schema.sdl(), "GraphQL schema definition");

        while let Some(msg) = inbound_relay.recv().await {
            let res = schema.execute(msg.req).await;
            msg.reply_channel.send(res).unwrap();
        }

        Ok(())
    }
}
