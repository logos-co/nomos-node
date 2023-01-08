mod backends;

// std
use std::fmt::{self, Debug};

// crates
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::{InboundRelay, NoMessage, RelayMessage},
    state::{NoOperator, NoState, ServiceState},
    ServiceCore, ServiceData, ServiceId,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast::Sender, oneshot::Receiver};

// internal
use backends::HttpServer;

#[derive(Serialize, Deserialize)]
pub struct Config<B: HttpServer> {
    pub backend: B::Config,
}

pub struct HttpService<B: HttpServer> {
    backend: B,
    inbound_relay: InboundRelay<HttpMsg<B>>,
}

impl<B: HttpServer> ServiceData for HttpService<B> {
    const SERVICE_ID: ServiceId = "Http";
    type Settings = Config<B>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = HttpMsg<B>;
}

pub enum HttpMsg<B: HttpServer> {
    AddHandler {
        service_id: ServiceId,
        route: String, // /metrics/ {GET/POST}
        rx: Sender<Receiver<B::Response>>,
    },
}

impl<B: HttpServer + 'static> RelayMessage for HttpMsg<B> {}

impl<B: HttpServer> Debug for HttpMsg<B> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::AddHandler {
                service_id,
                route,
                rx,
            } => write!(
                fmt,
                "HttpMsg::AddHandler{{ sender: {:?}, route: {:?}, rx: {:?} }}",
                service_id, route, rx
            ),
        }
    }
}

#[async_trait::async_trait]
impl<B: HttpServer> ServiceCore for HttpService<B> {
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        let inbound_relay = service_state.inbound_relay;
        Ok(Self {
            backend: <B as HttpServer>::new(
                service_state.settings_reader.get_updated_settings().backend,
            ),
            inbound_relay,
        })
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
        let Self {
            mut backend,
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

        loop {
            // select
            // handle the HttpMsg
            while let Some(HttpMsg::AddHandler {
                service_id,
                route,
                rx,
            }) = inbound_relay.recv().await
            {}
            // backend.run()
        }
        Ok(())
    }
}

impl<B: HttpServer> Clone for Config<B> {
    fn clone(&self) -> Self {
        Self {
            backend: self.backend.clone(),
        }
    }
}
