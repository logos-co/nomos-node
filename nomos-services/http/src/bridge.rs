use async_trait::async_trait;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::relay::NoMessage;
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use overwatch_rs::DynError;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::sync::Arc;

pub type HttpBridgeRunner =
    Box<dyn Future<Output = Result<(), overwatch_rs::DynError>> + Send + Unpin + 'static>;

pub type HttpBridgeBuilder = Arc<
    Box<
        dyn Fn(overwatch_rs::overwatch::handle::OverwatchHandle) -> HttpBridgeRunner
            + Send
            + Sync
            + 'static,
    >,
>;

pub struct HttpBridge {
    pub(crate) runners: Vec<HttpBridgeRunner>,
}

#[derive(Clone)]
pub struct HttpBridgeSettings {
    pub runners: Vec<HttpBridgeBuilder>,
}

impl Debug for HttpBridgeSettings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RouterSettings")
            .field("runners len", &self.runners.len())
            .finish()
    }
}

impl ServiceData for HttpBridge {
    const SERVICE_ID: ServiceId = "Router";
    type Settings = HttpBridgeSettings;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NoMessage;
}

#[async_trait]
impl ServiceCore for HttpBridge {
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let runners = service_state.settings_reader.get_updated_settings().runners;
        let runners: Vec<_> = runners
            .into_iter()
            .map(|r| (r)(service_state.overwatch_handle.clone()))
            .collect();
        Ok(Self { runners })
    }

    async fn run(self) -> Result<(), DynError> {
        futures::future::join_all(self.runners).await;
        Ok(())
    }
}
