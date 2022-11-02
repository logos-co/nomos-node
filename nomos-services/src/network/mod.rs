pub mod backends;

use async_trait::async_trait;
use backends::NetworkBackend;
use overwatch::services::{
    handle::ServiceStateHandle,
    relay::RelayMessage,
    state::{NoOperator, ServiceState},
    ServiceCore, ServiceData, ServiceId,
};
use std::fmt::Debug;
use tokio::sync::broadcast;
use tokio::sync::oneshot;

#[derive(Debug)]
pub enum NetworkMsg<T> {
    Broadcast(T),
    Subscribe {
        kind: EventKind,
        sender: oneshot::Sender<broadcast::Receiver<NetworkEvent<T>>>,
    },
}

impl<T: 'static> RelayMessage for NetworkMsg<T> {}

#[derive(Debug)]
pub enum EventKind {
    Message,
}

#[derive(Debug, Clone)]
pub enum NetworkEvent<T> {
    RawMessage(T),
}

pub struct NetworkConfig<I: NetworkBackend> {
    pub backend: I::Config,
}

pub struct NetworkService<I: NetworkBackend + Send + 'static> {
    backend: I,
    service_state: ServiceStateHandle<Self>,
}

pub struct NetworkState<I: NetworkBackend> {
    _backend: I::State,
}

impl<I: NetworkBackend + Send + 'static> ServiceData for NetworkService<I> {
    const SERVICE_ID: ServiceId = "Network";
    type Settings = NetworkConfig<I>;
    type State = NetworkState<I>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NetworkMsg<I::Message>;
}

#[async_trait]
impl<I: NetworkBackend + Send + 'static> ServiceCore for NetworkService<I> {
    fn init(mut service_state: ServiceStateHandle<Self>) -> Self {
        Self {
            backend: <I as NetworkBackend>::new(
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
                NetworkMsg::Broadcast(msg) => backend.broadcast(msg),
                NetworkMsg::Subscribe { kind, sender } => {
                    sender.send(backend.subscribe(kind)).unwrap_or_else(|_| {
                        tracing::warn!(
                            "client hung up before a subscription handle could be established"
                        )
                    })
                }
            }
        }
    }
}

impl<I: NetworkBackend> Clone for NetworkConfig<I> {
    fn clone(&self) -> Self {
        NetworkConfig {
            backend: self.backend.clone(),
        }
    }
}

impl<I: NetworkBackend> Clone for NetworkState<I> {
    fn clone(&self) -> Self {
        NetworkState {
            _backend: self._backend.clone(),
        }
    }
}

impl<I: NetworkBackend + Send + 'static> ServiceState for NetworkState<I> {
    type Settings = NetworkConfig<I>;

    fn from_settings(settings: &Self::Settings) -> Self {
        Self {
            _backend: I::State::from_settings(&settings.backend),
        }
    }
}
