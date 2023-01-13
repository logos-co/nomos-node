// std
use std::{collections::HashMap, convert::Infallible, sync::Arc};

// crates
use axum::{extract::Query, http::HeaderValue, routing::get, Router};
use hyper::{
    header::{CONTENT_TYPE, USER_AGENT},
    service::make_service_fn,
};
use overwatch_rs::{services::state::NoState, DynError};
use parking_lot::Mutex;
use tokio::sync::mpsc::Sender;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

// internal
use super::HttpBackend;
use crate::http::{HttpMethod, HttpRequest, Route};

/// Configuration for the Http Server
#[derive(Debug, Clone, clap::Args, serde::Deserialize, serde::Serialize)]
pub struct AxumBackendSettings {
    /// Socket where the server will be listening on for incoming requests.
    #[arg(
        short, long = "http-addr",
        default_value_t = std::net::SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
            8080,
        ),
        env = "HTTP_BIND_ADDRESS"
    )]
    pub address: std::net::SocketAddr,
    /// Allowed origins for this server deployment requests.
    #[arg(long = "http-cors-origin")]
    pub cors_origins: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum AxnumBackendError {
    #[error("axum backend: send error: {0}")]
    SendError(#[from] tokio::sync::mpsc::error::SendError<HttpRequest>),

    #[error("axum backend: {0}")]
    Any(DynError),
}

#[derive(Clone, Debug)]
pub struct AxumBackend {
    config: AxumBackendSettings,
    router: Arc<Mutex<Router>>,
}

#[async_trait::async_trait]
impl HttpBackend for AxumBackend {
    type Config = AxumBackendSettings;
    type State = NoState<AxumBackendSettings>;
    type Error = AxnumBackendError;

    fn new(config: Self::Config) -> Result<Self, Self::Error>
    where
        Self: Sized,
    {
        let mut builder = CorsLayer::new();
        if config.cors_origins.is_empty() {
            builder = builder.allow_origin(Any);
        }
        for origin in &config.cors_origins {
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

        let router = Router::new().layer(cors).layer(TraceLayer::new_for_http());
        let router = Arc::new(Mutex::new(router));

        Ok(Self { config, router })
    }

    fn add_route(
        &self,
        service_id: overwatch_rs::services::ServiceId,
        route: Route,
        req_stream: Sender<HttpRequest>,
    ) {
        let path = format!("/{}/{}", service_id.to_lowercase(), route.path);
        match route.method {
            HttpMethod::GET => self.add_get_route(&path, req_stream),
            _ => todo!(),
        };
    }

    async fn run(&self) -> Result<(), overwatch_rs::DynError> {
        let make_service = make_service_fn(|_| {
            let router = self.router.lock().clone();
            async move { Ok::<_, Infallible>(router) }
        });

        axum::Server::bind(&self.config.address)
            .serve(make_service)
            .await?;
        Ok(())
    }
}

impl AxumBackend {
    fn add_get_route(&self, path: &str, req_stream: Sender<HttpRequest>) {
        let mut router = self.router.lock();
        *router = router.clone().route(
            path,
            // TODO: Extract the stream handling to `to_handler` or similar function.
            get(|Query(query): Query<HashMap<String, String>>| async move {
                let (tx, mut rx) = tokio::sync::mpsc::channel(1);

                // Write Self::Request type message to req_stream.
                // TODO: handle result in a more elegant way.
                // Currently, convert to Result<String, String>
                match req_stream
                    .send(HttpRequest {
                        query,
                        payload: None,
                        res_tx: tx,
                    })
                    .await
                {
                    Ok(_) => {
                        // Wait for a response, then pass or serialize it?
                        match rx.recv().await {
                            Some(res) => Ok(res),
                            None => Ok("".into()),
                        }
                    }
                    Err(e) => Err(AxnumBackendError::SendError(e).to_string()),
                }
            }),
        )
    }
}
