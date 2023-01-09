// std
use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
};

// crates
use axum::{extract::Query, routing::get, Router};
use overwatch_rs::services::state::NoState;
use serde_json::Value;
use tokio::sync::{
    broadcast::{self, Sender},
    oneshot::{self, Receiver},
};

// internal
use super::HttpBackend;
use crate::{HttpMethod, HttpRequest};

/// Configuration for the Http Server
#[derive(Debug, Clone, clap::Args, serde::Deserialize, serde::Serialize)]
pub struct AxumBackendSettings {
    /// Socket where the server will be listening on for incoming requests.
    #[arg(short, long = "addr", default_value_t = std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)), 8080), env = "HTTP_ADDRESS")]
    pub address: std::net::SocketAddr,
    /// Allowed origins for this server deployment requests.
    #[arg(long = "cors-origin")]
    pub cors_origins: Vec<String>,
}

pub trait HandlerOutput<T>: Future<Output = T> + Sized + Send {}

#[derive(Clone, Debug)]
pub struct AxumBackend {
    config: AxumBackendSettings,
    router: Arc<Mutex<Router>>,
}

#[async_trait::async_trait]
impl HttpBackend for AxumBackend {
    type Config = AxumBackendSettings;
    type State = NoState<AxumBackendSettings>;
    type Request = ();
    type Response = String;

    fn new(config: Self::Config) -> Result<Self, overwatch_rs::DynError>
    where
        Self: Sized,
    {
        Ok(Self {
            config,
            router: Default::default(),
        })
    }

    fn add_route(
        &self,
        service_id: overwatch_rs::services::ServiceId,
        route: crate::Route,
        req_stream: Sender<HttpRequest<Self::Request, Self::Response>>,
    ) {
        let router = &mut *self.router.lock().unwrap();
        let path = format!("/{}/{}", service_id, route.path);
        match route.method {
            HttpMethod::GET => {
                *router = router.clone().route(
                    &path,
                    // TODO: Extract the stream handling to `to_handler` or similar function.
                    get(|Query(query): Query<HashMap<String, String>>| async move {
                        let (tx, mut rx) = broadcast::channel(1);

                        // Write Self::Request type message to req_stream.
                        // TODO: handle result.
                        req_stream.send(HttpRequest {
                            query,
                            payload: (),
                            res_tx: tx,
                        });

                        // Wait for a response, then pass or serialize it?
                        match rx.recv().await {
                            Ok(res) => res,
                            Err(err) => err.to_string(),
                        }
                    }),
                )
            }
            _ => todo!(),
        };
    }

    async fn run(&self) -> Result<(), overwatch_rs::DynError> {
        let router = self.router.lock().unwrap().clone();
        axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
            .serve(router.into_make_service())
            .await?;
        Ok(())
    }
}
