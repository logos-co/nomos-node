#[cfg(feature = "http")]
pub mod axum;

use std::fmt::Debug;

use overwatch_rs::{
    services::{state::ServiceState, ServiceId},
    DynError,
};

use crate::Route;

#[async_trait::async_trait]
pub trait HttpBackend: Send + Sync + 'static {
    type Config: Clone + Debug + Send + Sync + 'static;
    type State: ServiceState<Settings = Self::Config> + Clone;
    type Request: Send;
    type Response: Send;
    type Handler: Send + Sync + Sized;

    fn new(config: Self::Config) -> Result<Self, DynError>
    where
        Self: Sized;
    fn add_route(&self, service_id: ServiceId, route: Route<Self>)
    where
        Self: Sized;
    async fn run(&self) -> Result<(), overwatch_rs::DynError>;
}
