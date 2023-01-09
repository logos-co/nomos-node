#[cfg(feature = "http")]
pub mod axum;

use std::fmt::Debug;

use overwatch_rs::{
    services::{state::ServiceState, ServiceId},
    DynError,
};
use tokio::sync::broadcast::Sender;

use crate::{HttpRequest, Route};

#[async_trait::async_trait]
pub trait HttpBackend: Send + Sync + 'static {
    type Config: Clone + Debug + Send + Sync + 'static;
    type State: ServiceState<Settings = Self::Config> + Clone;
    type Request: Clone + Send;
    type Response: Clone + Send;

    fn new(config: Self::Config) -> Result<Self, DynError>
    where
        Self: Sized;
    fn add_route(
        &self,
        service_id: ServiceId,
        route: Route,
        req_stream: Sender<HttpRequest<Self::Request, Self::Response>>,
    ) where
        Self: Sized;
    async fn run(&self) -> Result<(), overwatch_rs::DynError>;
}
