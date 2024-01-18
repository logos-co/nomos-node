use overwatch_rs::{
    overwatch::handle::OverwatchHandle,
    services::{
        handle::ServiceStateHandle,
        relay::NoMessage,
        state::{NoOperator, NoState},
        ServiceCore, ServiceData,
    },
    DynError,
};
use std::future::Future;

pub mod http;

/// A simple abstraction so that we can easily
/// change the underlying http server
pub trait Backend {
    type Error: std::error::Error + Send + Sync + 'static;
    type Settings: Clone + Send + Sync + 'static;

    fn new(settings: Self::Settings) -> impl Future<Output = Result<Self, Self::Error>> + Send
    where
        Self: Sized;

    fn serve(self, handle: OverwatchHandle)
        -> impl Future<Output = Result<(), Self::Error>> + Send;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApiServiceSettings<S> {
    pub backend_settings: S,
}

pub struct ApiService<B: Backend> {
    settings: ApiServiceSettings<B::Settings>,
    handle: OverwatchHandle,
}

impl<B: Backend> ServiceData for ApiService<B> {
    const SERVICE_ID: overwatch_rs::services::ServiceId = "nomos-api";

    type Settings = ApiServiceSettings<B::Settings>;

    type State = NoState<Self::Settings>;

    type StateOperator = NoOperator<Self::State>;

    type Message = NoMessage;
}

impl<B> ServiceCore for ApiService<B>
where
    B: Backend + Send + Sync + 'static,
{
    /// Initialize the service with the given state
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let settings = service_state.settings_reader.get_updated_settings();
        Ok(Self {
            settings,
            handle: service_state.overwatch_handle,
        })
    }

    /// Service main loop
    async fn run(self) -> Result<(), DynError> {
        let endpoint = B::new(self.settings.backend_settings).await?;
        endpoint.serve(self.handle).await?;
        Ok(())
    }
}
