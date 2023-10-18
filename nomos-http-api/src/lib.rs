use overwatch_rs::{
    services::{
        handle::ServiceStateHandle,
        relay::NoMessage,
        state::{NoOperator, NoState},
        ServiceCore, ServiceData,
    },
    DynError,
};

pub mod http;

/// A simple abstraction so that we can easily
/// change the underlying http server
#[async_trait::async_trait]
pub trait Backend {
    type Error: std::error::Error + Send + Sync + 'static;
    type Settings: Clone + Send + Sync + 'static;

    async fn new(settings: Self::Settings) -> Result<Self, Self::Error>
    where
        Self: Sized;

    async fn serve(self) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone)]
pub struct ApiServiceSettings<S> {
    pub backend_settings: S,
}

pub struct ApiService<B: Backend> {
    settings: ApiServiceSettings<B::Settings>,
}

impl<B: Backend> ServiceData for ApiService<B> {
    const SERVICE_ID: overwatch_rs::services::ServiceId = "nomos-api";

    type Settings = ApiServiceSettings<B::Settings>;

    type State = NoState<Self::Settings>;

    type StateOperator = NoOperator<Self::State>;

    type Message = NoMessage;
}

#[async_trait::async_trait]
impl<B> ServiceCore for ApiService<B>
where
    B: Backend + Send + Sync + 'static,
{
    /// Initialize the service with the given state
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let settings = service_state.settings_reader.get_updated_settings();
        Ok(Self { settings })
    }

    /// Service main loop
    async fn run(mut self) -> Result<(), DynError> {
        let endpoint = B::new(self.settings.backend_settings).await?;
        endpoint.serve().await?;
        Ok(())
    }
}

#[test]
fn test_service_() {}
