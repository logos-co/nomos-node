pub mod backends;

// std
use std::{
    collections::HashMap,
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
use backends::HttpBackend;

#[derive(Serialize, Deserialize, Debug)]
pub struct Config<B: HttpBackend> {
    pub backend: B::Config,
}

pub struct HttpService<B: HttpBackend> {
    backend: B,
    inbound_relay: InboundRelay<HttpMsg<B>>,
}

impl<B: HttpBackend> ServiceData for HttpService<B> {
    const SERVICE_ID: ServiceId = "Http";
    type Settings = Config<B>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = HttpMsg<B>;
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
pub struct HttpRequest<Req, Res> {
    pub query: HashMap<String, String>,
    pub payload: Req,
    pub res_tx: Sender<Res>,
}

pub enum HttpMsg<B: HttpBackend> {
    AddHandler {
        service_id: ServiceId,
        route: Route,
        req_stream: Sender<HttpRequest<B::Request, B::Response>>,
    },
}

impl<B: HttpBackend + 'static> RelayMessage for HttpMsg<B> {}

impl<B: HttpBackend> Debug for HttpMsg<B> {
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
impl<B: HttpBackend> ServiceCore for HttpService<B> {
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        let inbound_relay = service_state.inbound_relay;
        <B as HttpBackend>::new(service_state.settings_reader.get_updated_settings().backend).map(
            |backend| Self {
                backend,
                inbound_relay,
            },
        )
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
        let Self {
            backend,
            mut inbound_relay,
        } = self;

        // TODO: not quite sure if this will restart the server every time
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
                _server_exit = backend.run() => {
                    todo!()
                }
            }
        }

        // let backend = Arc::new(backend);
        // let (stop_tx, mut stop_rx) = oneshot::channel();
        // let tbc = backend.clone();
        // tokio::spawn(async move {
        //     loop {
        //         tokio::select! {
        //             Some(msg) = inbound_relay.recv() => {
        //                 match msg {
        //                     HttpMsg::AddHandler {
        //                         service_id,
        //                         route,
        //                         req_stream,
        //                     } => {
        //                         tbc.add_route(service_id, route, req_stream);
        //                     }
        //                 }
        //             }
        //             _server_exit = &mut stop_rx => {
        //                 break;
        //             }
        //         }
        //     }
        // });
        // backend
        //     .run()
        //     .await
        //     .map_err(|e| {
        //         if stop_tx.send(()).is_err() {
        //             tracing::error!("HTTP service: failed to send stop signal to HTTP backend.");
        //         }
        //         e
        //     })
    }
}

impl<B: HttpBackend> Clone for Config<B> {
    fn clone(&self) -> Self {
        Self {
            backend: self.backend.clone(),
        }
    }
}
