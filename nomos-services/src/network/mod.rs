pub mod backends;

use async_trait::async_trait;
use backends::NetworkBackend;
use overwatch::services::{
    handle::ServiceStateHandle,
    relay::RelayMessage,
    state::{NoOperator, ServiceState},
    ServiceCore, ServiceData, ServiceId,
};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Debug};
use tokio::sync::broadcast;
use tokio::sync::oneshot;

pub enum NetworkMsg<B: NetworkBackend> {
    Process(B::Message),
    Subscribe {
        kind: B::EventKind,
        sender: oneshot::Sender<broadcast::Receiver<B::NetworkEvent>>,
    },
}

impl<B: NetworkBackend> Debug for NetworkMsg<B> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Process(msg) => write!(fmt, "NetworkMsg::Process({:?})", msg),
            Self::Subscribe { kind, sender } => write!(
                fmt,
                "NetworkMsg::Subscribe{{ kind: {:?}, sender: {:?}}}",
                kind, sender
            ),
        }
    }
}

impl<T: NetworkBackend + 'static> RelayMessage for NetworkMsg<T> {}

#[derive(Serialize, Deserialize)]
pub struct NetworkConfig<B: NetworkBackend> {
    pub backend: B::Config,
}

impl<B: NetworkBackend> Debug for NetworkConfig<B> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "NetworkConfig {{ backend: {:?}}}", self.backend)
    }
}

pub struct NetworkService<B: NetworkBackend + Send + 'static> {
    backend: B,
    service_state: ServiceStateHandle<Self>,
}

pub struct NetworkState<B: NetworkBackend> {
    _backend: B::State,
}

impl<B: NetworkBackend + Send + 'static> ServiceData for NetworkService<B> {
    const SERVICE_ID: ServiceId = "Network";
    type Settings = NetworkConfig<B>;
    type State = NetworkState<B>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NetworkMsg<B>;
}

#[async_trait]
impl<B: NetworkBackend + Send + 'static> ServiceCore for NetworkService<B> {
    fn init(mut service_state: ServiceStateHandle<Self>) -> Self {
        Self {
            backend: <B as NetworkBackend>::new(
                service_state.settings_reader.get_updated_settings().backend,
            ),
            service_state,
        }
    }

    async fn run(mut self) {
        let Self {
            service_state: ServiceStateHandle {
                mut inbound_relay, ..
            },
            mut backend,
        } = self;

        while let Some(msg) = inbound_relay.recv().await {
            match msg {
                NetworkMsg::Process(msg) => {
                    // split sending in two steps to help the compiler understand we do not
                    // need to hold an instance of &I (which is not send) across an await point
                    let _send = backend.process(msg);
                    _send.await
                }
                NetworkMsg::Subscribe { kind, sender } => sender
                    .send(backend.subscribe(kind).await)
                    .unwrap_or_else(|_| {
                        tracing::warn!(
                            "client hung up before a subscription handle could be established"
                        )
                    }),
            }
        }
    }
}

impl<B: NetworkBackend> Clone for NetworkConfig<B> {
    fn clone(&self) -> Self {
        NetworkConfig {
            backend: self.backend.clone(),
        }
    }
}

impl<B: NetworkBackend> Clone for NetworkState<B> {
    fn clone(&self) -> Self {
        NetworkState {
            _backend: self._backend.clone(),
        }
    }
}

impl<B: NetworkBackend + Send + 'static> ServiceState for NetworkState<B> {
    type Settings = NetworkConfig<B>;

    fn from_settings(settings: &Self::Settings) -> Self {
        Self {
            _backend: B::State::from_settings(&settings.backend),
        }
    }
}
