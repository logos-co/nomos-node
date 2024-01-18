#[cfg(feature = "http")]
pub mod axum;

use std::fmt::Debug;

use futures::Future;
use overwatch_rs::services::{state::ServiceState, ServiceId};
use tokio::sync::mpsc::Sender;

use crate::http::{HttpRequest, Route};

pub trait HttpBackend {
    type Settings: Clone + Debug + Send + Sync + 'static;
    type State: ServiceState<Settings = Self::Settings> + Clone;
    type Error: std::fmt::Display;

    fn new(config: Self::Settings) -> Result<Self, Self::Error>
    where
        Self: Sized;

    fn add_route(&self, service_id: ServiceId, route: Route, req_stream: Sender<HttpRequest>);

    fn run(&self) -> impl Future<Output = Result<(), overwatch_rs::DynError>> + Send;
}
