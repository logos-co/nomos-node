// std

// crates
use axum::{
    http::{
        header::{CONTENT_TYPE, USER_AGENT},
        HeaderValue,
    },
    routing::{post, get},
    Router, Server, response::Html,
};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

// internal
use overwatch_rs::services::relay::Relay;
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::NoMessage,
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};
/// Configuration for the Http Server
#[derive(Debug, Clone, clap::Args, serde::Deserialize, serde::Serialize)]
pub struct ServerSettings {
    /// Socket where the server will be listening on for incoming requests.
    #[arg(short, long = "addr", default_value_t = std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)), 8080), env = "HTTP_ADDRESS")]
    pub address: std::net::SocketAddr,
    /// Allowed origins for this server deployment requests.
    #[arg(long = "cors-origin")]
    pub cors_origins: Vec<String>,
}

pub trait Data {

}

pub trait Abstration {
    
}

#[derive(Clone)]
pub struct HttpServer<HTTPAbstration: ServiceCore> {
    settings: ServerSettings,
    http_abstraction_channel: Relay<HTTPAbstration>,
}

impl<Backend: ServiceCore + Send + Sync + 'static> ServiceData for HttpServer<Backend> {
    const SERVICE_ID: ServiceId = "HttpServer";

    type Settings = ServerSettings;

    type State = NoState<ServerSettings>;

    type StateOperator = NoOperator<Self::State>;

    type Message = NoMessage;
}

async fn handler() -> Html<&'static str> {
    Html("<h1>Hello, World!</h1>")
}

#[async_trait::async_trait]
impl<HTTPAbstration: ServiceCore + Send + Sync + 'static> ServiceCore for HttpServer<HTTPAbstration>
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        let settings = service_state.settings_reader.get_updated_settings();
        let http_abstraction_channel: Relay<HTTPAbstration> =
            service_state.overwatch_handle.relay();
        Ok(Self {
            settings,
            http_abstraction_channel,
        })
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
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
            .route("/", get(handler))
            .layer(cors)
            .layer(TraceLayer::new_for_http());

        tracing::info!("HTTP server listening: {}", addr);
        let mut srv = Server::bind(&addr).serve(router.into_make_service());
        let conn = self.http_abstraction_channel.connect().await?;
        
        Ok(())
    }
}