// std
use std::{
    collections::HashMap,
    error::Error,
    fmt::{self, Debug},
    sync::Arc,
};

// crates
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::{InboundRelay, RelayMessage},
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc::Sender, oneshot};

// internal
use crate::backends::HttpBackend;

#[derive(Serialize, Deserialize, Debug)]
pub struct Config<B: HttpBackend> {
    pub backend: B::Config,
}

pub struct HttpService<B: HttpBackend> {
    backend: B,
    inbound_relay: InboundRelay<HttpMsg>,
}

impl<B: HttpBackend + 'static> ServiceData for HttpService<B> {
    const SERVICE_ID: ServiceId = "Http";
    type Settings = Config<B>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = HttpMsg;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    PATCH,
    DELETE,
}

#[derive(PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Route {
    pub method: HttpMethod,
    pub path: String,
}

impl core::fmt::Debug for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Route")
            .field("method", &self.method)
            .field("path", &self.path)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub query: HashMap<String, String>,
    pub payload: Option<bytes::Bytes>,
    pub res_tx: Sender<bytes::Bytes>,
}

// HttpMsg is a message that is sent via the relay to communicate with
// the HttpService.
pub enum HttpMsg {
    AddHandler {
        // TODO: If possible, this should be extracted from the
        // relay itself.
        service_id: ServiceId,
        route: Route,
        req_stream: Sender<HttpRequest>,
    },
}

impl HttpMsg {
    pub fn add_get_handler<P: Into<String>>(
        service_id: ServiceId,
        path: P,
        req_stream: Sender<HttpRequest>,
    ) -> Self {
        Self::AddHandler {
            service_id,
            route: Route {
                method: HttpMethod::GET,
                path: path.into(),
            },
            req_stream,
        }
    }
}

impl RelayMessage for HttpMsg {}

impl Debug for HttpMsg {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::AddHandler {
                service_id,
                route,
                req_stream: _,
            } => write!(
                fmt,
                "HttpMsg::AddHandler {{ sender: {:?}, route: {:?} }}",
                service_id, route
            ),
        }
    }
}

#[async_trait::async_trait]
impl<B> ServiceCore for HttpService<B>
where
    B: HttpBackend + Send + Sync + 'static,
    <B as HttpBackend>::Error: Error + Send + Sync + 'static,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        let inbound_relay = service_state.inbound_relay;
        <B as HttpBackend>::new(service_state.settings_reader.get_updated_settings().backend)
            .map(|backend| Self {
                backend,
                inbound_relay,
            })
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
        let Self {
            backend,
            mut inbound_relay,
        } = self;

        let backend = Arc::new(backend);
        let (stop_tx, mut stop_rx) = oneshot::channel();
        tokio::spawn({
            let backend = backend.clone();
            async move {
                loop {
                    tokio::select! {
                        Some(msg) = inbound_relay.recv() => {
                            match msg {
                                HttpMsg::AddHandler {
                                    service_id,
                                    route,
                                    req_stream,
                                } => {
                                    backend.add_route(service_id, route, req_stream);
                                }
                            }
                        }
                        _server_exit = &mut stop_rx => {
                            break;
                        }
                    }
                }
            }
        });
        backend.run().await.map_err(|e| {
            if stop_tx.send(()).is_err() {
                tracing::error!("HTTP service: failed to send stop signal to HTTP backend.");
            }
            e
        })
    }
}

impl<B: HttpBackend> Clone for Config<B> {
    fn clone(&self) -> Self {
        Self {
            backend: self.backend.clone(),
        }
    }
}
