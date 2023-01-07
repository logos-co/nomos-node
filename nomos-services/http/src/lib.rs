mod backends;

// std

// crates
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::NoMessage,
    state::{NoOperator, ServiceState},
    ServiceCore, ServiceData, ServiceId,
};
use serde::{Deserialize, Serialize};

// internal
use backends::HttpServer;

#[derive(Serialize, Deserialize)]
pub struct Config<B: HttpServer> {
    pub backend: B::Config,
}

pub struct State<B: HttpServer> {
    _backend: B::State,
}

pub struct HttpService<B: HttpServer> {
    backend: B,
    service_state: ServiceStateHandle<Self>,
}

impl<B: HttpServer> ServiceData for HttpService<B> {
    const SERVICE_ID: ServiceId = "Http";
    type Settings = Config<B>;
    type State = State<B>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NoMessage;
}

#[async_trait::async_trait]
impl<B: HttpServer> ServiceCore for HttpService<B> {
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        Ok(Self {
            backend: <B as HttpServer>::new(
                service_state.settings_reader.get_updated_settings().backend,
            ),
            service_state,
        })
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
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

impl<B: HttpServer> Clone for State<B> {
    fn clone(&self) -> Self {
        Self {
            _backend: self._backend.clone(),
        }
    }
}

impl<B: HttpServer> ServiceState for State<B> {
    type Settings = Config<B>;
    type Error = <B::State as ServiceState>::Error;

    fn from_settings(settings: &Self::Settings) -> Result<Self, Self::Error> {
        B::State::from_settings(&settings.backend).map(|_backend| Self { _backend })
    }
}
