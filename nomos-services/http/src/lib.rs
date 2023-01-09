mod backends;

// std
use std::fmt::{self, Debug};

// crates
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::{InboundRelay, RelayMessage},
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot::{Receiver, Sender};

// internal
use backends::HttpBackend;

#[derive(Serialize, Deserialize)]
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
pub struct Route<B: HttpBackend> {
    pub method: HttpMethod,
    pub path: String,
    pub handler: B::Handler,
}

impl<B: HttpBackend> core::fmt::Debug for Route<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Route")
            .field("method", &self.method)
            .field("path", &self.path)
            .finish()
    }
}

pub enum HttpMsg<B: HttpBackend> {
    AddHandler {
        service_id: ServiceId,
        route: Route<B>, // /metrics/ {GET/POST}
        tx: Sender<Receiver<B::Response>>,
    },
}

impl<B: HttpBackend + 'static> RelayMessage for HttpMsg<B> {}

impl<B: HttpBackend> Debug for HttpMsg<B> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::AddHandler {
                service_id,
                route,
                tx: _,
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

        // 1.
        // Create router.
        // Pass it to the backend.
        //
        // 2.
        // Get a router object.
        // Store it in this scope.

        // backend.add_route(todo!());

        while let Some(msg) = inbound_relay.recv().await {
            match msg {
                HttpMsg::AddHandler {
                    service_id,
                    route,
                    // TODO: discuss do we actually need this sender.
                    tx: _,
                } => {
                    // create route
                    backend.add_route(service_id, route);
                }
            }
        }
        Ok(())
    }
}

impl<B: HttpBackend> Clone for Config<B> {
    fn clone(&self) -> Self {
        Self {
            backend: self.backend.clone(),
        }
    }
}
