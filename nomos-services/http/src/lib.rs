// std

// crates
use axum::{
    http::{
        header::{CONTENT_TYPE, USER_AGENT},
        HeaderValue,
    },
    response::Html,
    routing::get,
    Router, Server,
};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

// internal
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

#[derive(Clone)]
pub struct HttpRouter {
    settings: ServerSettings,
}

impl ServiceData for HttpRouter {
    const SERVICE_ID: ServiceId = "HttpRouter";

    type Settings = ServerSettings;

    type State = NoState<ServerSettings>;

    type StateOperator = NoOperator<Self::State>;

    type Message = NoMessage;
}

async fn handler() -> Html<&'static str> {
    Html("<h1>Hello, World!</h1>")
}

#[async_trait::async_trait]
impl ServiceCore for HttpRouter {
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        let settings = service_state.settings_reader.get_updated_settings();
        Ok(Self { settings })
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
        Server::bind(&addr)
            .serve(router.into_make_service())
            .await?;

        Ok(())
    }
}
