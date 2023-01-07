use std::fmt::Debug;

use overwatch_rs::services::state::ServiceState;

#[cfg(feature = "http")]
pub mod axum;

#[async_trait::async_trait]
pub trait HttpServer: Send + Sync + 'static {
    type Config: Clone + Debug + Send + Sync + 'static;
    type State: ServiceState<Settings = Self::Config> + Clone;
    type Message: Debug + Send + Sync + 'static;

    fn new(config: Self::Config) -> Self;
}
