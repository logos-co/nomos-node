pub use prometheus_client::{self, *};

// std
use std::fmt::{Debug, Error, Formatter};
use std::sync::{Arc, Mutex};
// crates
use futures::StreamExt;
use overwatch_rs::services::life_cycle::LifecycleMessage;
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::RelayMessage,
    state::{NoOperator, NoState},
    ServiceCore, ServiceData,
};
use prometheus_client::encoding::text::encode;
use prometheus_client::registry::Registry;
use tokio::sync::oneshot::Sender;
use tracing::error;
// internal

// A wrapper for prometheus_client Registry.
// Lock is only used during services initialization and prometheus pull query.
pub type NomosRegistry = Arc<Mutex<Registry>>;

pub struct Metrics {
    service_state: ServiceStateHandle<Self>,
    registry: NomosRegistry,
}

#[derive(Clone, Debug)]
pub struct MetricsSettings {
    pub registry: Option<NomosRegistry>,
}

pub enum MetricsMsg {
    Gather { reply_channel: Sender<String> },
}

impl RelayMessage for MetricsMsg {}

impl Debug for MetricsMsg {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match self {
            Self::Gather { .. } => {
                write!(f, "MetricsMsg::Gather")
            }
        }
    }
}

impl ServiceData for Metrics {
    const SERVICE_ID: &'static str = "Metrics";
    type Settings = MetricsSettings;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = MetricsMsg;
}

#[async_trait::async_trait]
impl ServiceCore for Metrics {
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        let config = service_state.settings_reader.get_updated_settings();

        Ok(Self {
            service_state,
            registry: config.registry.ok_or("No registry provided")?,
        })
    }

    async fn run(self) -> Result<(), overwatch_rs::DynError> {
        let Self {
            mut service_state,
            registry,
        } = self;
        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();
        loop {
            tokio::select! {
                Some(msg) = service_state.inbound_relay.recv() => {
                    let MetricsMsg::Gather{reply_channel} = msg;

                    let mut buf = String::new();
                    {
                        let reg = registry.lock().unwrap();
                        // If encoding fails, we need to stop trying process subsequent metrics gather
                        // requests. If it succeds, encode method returns empty unit type.
                        _ = encode(&mut buf, &reg).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));
                    }

                    reply_channel
                        .send(buf)
                        .unwrap_or_else(|_| tracing::debug!("could not send back metrics"));
                }
                Some(msg) = lifecycle_stream.next() =>  {
                    if Self::should_stop_service(msg).await {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}

impl Metrics {
    async fn should_stop_service(message: LifecycleMessage) -> bool {
        match message {
            LifecycleMessage::Shutdown(sender) => {
                if sender.send(()).is_err() {
                    error!(
                        "Error sending successful shutdown signal from service {}",
                        Self::SERVICE_ID
                    );
                }
                true
            }
            LifecycleMessage::Kill => true,
        }
    }
}
