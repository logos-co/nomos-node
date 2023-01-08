#[cfg(feature = "http")]
pub mod axum;

use std::fmt::Debug;

use overwatch_rs::services::state::ServiceState;

#[async_trait::async_trait]
pub trait HttpServer: Send + Sync + 'static {
    type Config: Clone + Debug + Send + Sync + 'static;
    type State: ServiceState<Settings = Self::Config> + Clone;
    type Request: Send;
    type Response: Send;

    fn new(config: Self::Config) -> Self;
    fn add_route(&self);
    async fn run(&self) -> Result<(), overwatch_rs::DynError>;
}
