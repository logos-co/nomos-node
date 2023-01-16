#[cfg(feature = "http")]
pub mod axum;

use std::fmt::Debug;

use overwatch_rs::services::{state::ServiceState, ServiceId};
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::mpsc::Sender;

use crate::http::{HttpRequest, Route};

pub type Payload = Box<dyn DeserializeOwned>;
pub type Response = Box<dyn Serialize>;

#[async_trait::async_trait]
pub trait HttpBackend {
    type Config: Clone + Debug + Send + Sync + 'static;
    type State: ServiceState<Settings = Self::Config> + Clone;
    type Error: std::fmt::Display;

    fn new(config: Self::Config) -> Result<Self, Self::Error>
    where
        Self: Sized;

    fn add_route(&self, service_id: ServiceId, route: Route, req_stream: Sender<HttpRequest>);

    async fn run(&self) -> Result<(), overwatch_rs::DynError>;
}
