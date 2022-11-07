pub mod backends;

use async_trait::async_trait;
use backends::NetworkBackend;
use overwatch::services::{
    handle::ServiceStateHandle,
    relay::RelayMessage,
    state::{NoOperator, ServiceState},
    ServiceCore, ServiceData, ServiceId,
};
use std::fmt::{self, Debug};
use tokio::sync::broadcast;
use tokio::sync::oneshot;

pub enum NetworkMsg<B: NetworkBackend> {
    Send(B::Message),
    Subscribe {
        kind: B::EventKind,
        sender: oneshot::Sender<broadcast::Receiver<B::NetworkEvent>>,
    },
}

impl<B: NetworkBackend> Debug for NetworkMsg<B> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Send(msg) => write!(fmt, "NetworkMsg::Send({:?})", msg),
            Self::Subscribe { kind, sender } => write!(
                fmt,
                "NetworkMsg::Subscribe{{ kind: {:?}, sender: {:?}}}",
                kind, sender
            ),
        }
    }
}

impl<T: NetworkBackend + 'static> RelayMessage for NetworkMsg<T> {}

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

impl<B: NetworkBackend + Send + 'static> ServiceData for NetworkService<B> {
    const SERVICE_ID: ServiceId = "Network";
    type Settings = NetworkConfig<B>;
    type State = NetworkState<B>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NetworkMsg<B>;
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
                NetworkMsg::Send(msg) => {
                    // split sending in two steps to help the compiler understand we do not
                    // need to hold an instance of &I (which is not send) across an await point
                    let _send = backend.send(msg);
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
