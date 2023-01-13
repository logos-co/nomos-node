use async_trait::async_trait;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::relay::NoMessage;
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use overwatch_rs::DynError;
use std::future::Future;
use std::sync::Arc;

type HttpRunner =
    Box<dyn Future<Output = Result<(), overwatch_rs::DynError>> + Send + Sync + Unpin + 'static>;

type Runner = Arc<
    Box<
        dyn Fn(&overwatch_rs::overwatch::handle::OverwatchHandle) -> HttpRunner
            + Send
            + Sync
            + 'static,
    >,
>;

pub struct Router {
    pub(crate) runners: Vec<HttpRunner>,
}

#[derive(Clone)]
pub struct RouterSettings {
    pub runners: Vec<Runner>,
}

impl ServiceData for Router {
    const SERVICE_ID: ServiceId = "Router";
    type Settings = RouterSettings;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NoMessage;
}

#[async_trait]
impl ServiceCore for Router {
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let runners = service_state.settings_reader.get_updated_settings().runners;
        let runners: Vec<_> = runners
            .into_iter()
            .map(|r| (r)(&service_state.overwatch_handle))
            .collect();
        Ok(Self { runners })
    }

    async fn run(self) -> Result<(), DynError> {
        futures::future::join_all(self.runners).await;
        Ok(())
    }
}
