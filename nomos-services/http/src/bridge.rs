use std::error::Error;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::sync::Arc;

use async_trait::async_trait;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::relay::{NoMessage, OutboundRelay};
use overwatch_rs::services::{
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};
use overwatch_rs::DynError;
use tokio::sync::mpsc::{channel, Receiver};

use crate::backends::HttpBackend;
use crate::http::{HttpMethod, HttpMsg, HttpRequest, HttpService};

pub type HttpBridgeRunner =
    Box<dyn Future<Output = Result<(), overwatch_rs::DynError>> + Send + Unpin + 'static>;

// TODO: If we can get rid of the clone bound on here remove Arc.
// For now as we bind this through the settings we need to keep it.
pub type HttpBridge = Arc<
    Box<
        dyn Fn(overwatch_rs::overwatch::handle::OverwatchHandle) -> HttpBridgeRunner
            + Send
            + Sync
            + 'static,
    >,
>;

// TODO: Add error handling
pub async fn build_http_bridge<S, B, P>(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
    method: HttpMethod,
    path: P,
) -> Result<(OutboundRelay<S::Message>, Receiver<HttpRequest>), overwatch_rs::DynError>
where
    S: ServiceCore + Send + Sync + 'static,
    B: HttpBackend + Send + Sync + 'static,
    B::Error: Error + Send + Sync + 'static,
    P: Into<String> + Send + Sync + 'static,
{
    let http_relay = handle.clone().relay::<HttpService<B>>().connect().await?;

    let service_relay = handle.clone().relay::<S>().connect().await?;

    let (http_sender, http_receiver) = channel(1);

    // Register on http service to receive GET requests.
    http_relay
        .send(HttpMsg::add_http_handler(
            method,
            S::SERVICE_ID,
            path,
            http_sender,
        ))
        .await
        .map_err(|(e, _)| e)?;

    Ok((service_relay, http_receiver))
}

pub struct HttpBridgeService {
    pub(crate) runners: Vec<HttpBridgeRunner>,
}

#[derive(Clone)]
pub struct HttpBridgeSettings {
    pub bridges: Vec<HttpBridge>,
}

impl Debug for HttpBridgeSettings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RouterSettings")
            .field("runners len", &self.bridges.len())
            .finish()
    }
}

impl ServiceData for HttpBridgeService {
    const SERVICE_ID: ServiceId = "Router";
    type Settings = HttpBridgeSettings;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NoMessage;
}

#[async_trait]
impl ServiceCore for HttpBridgeService {
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let runners = service_state.settings_reader.get_updated_settings().bridges;
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
