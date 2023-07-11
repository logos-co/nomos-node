pub mod backends;
// std
use std::fmt::{self, Debug};

// crates
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::sync::oneshot;
// internal
use backends::NetworkBackend;
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::RelayMessage,
    state::{NoOperator, ServiceState},
    ServiceCore, ServiceData, ServiceId,
};

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
            Self::Process(msg) => write!(fmt, "NetworkMsg::Process({msg:?})"),
            Self::Subscribe { kind, sender } => write!(
                fmt,
                "NetworkMsg::Subscribe{{ kind: {kind:?}, sender: {sender:?}}}"
            ),
        }
    }
}

impl<T: NetworkBackend + 'static> RelayMessage for NetworkMsg<T> {}

#[derive(Serialize, Deserialize)]
pub struct NetworkConfig<B: NetworkBackend> {
    pub backend: B::Settings,
}

impl<B: NetworkBackend> Debug for NetworkConfig<B> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "NetworkConfig {{ backend: {:?}}}", self.backend)
    }
}

pub struct NetworkService<B: NetworkBackend + 'static> {
    backend: B,
    service_state: ServiceStateHandle<Self>,
}

pub struct NetworkState<B: NetworkBackend> {
    _backend: B::State,
}

impl<B: NetworkBackend + 'static> ServiceData for NetworkService<B> {
    const SERVICE_ID: ServiceId = "Network";
    type Settings = NetworkConfig<B>;
    type State = NetworkState<B>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NetworkMsg<B>;
}

#[async_trait]
impl<B> ServiceCore for NetworkService<B>
where
    B: NetworkBackend + Send + 'static,
    B::State: Send + Sync,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        Ok(Self {
            backend: <B as NetworkBackend>::new(
                service_state.settings_reader.get_updated_settings().backend,
                service_state.overwatch_handle.clone(),
            ),
            service_state,
        })
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
        tracing::debug!("Starting up...");
        // this wait seems to be helpful in some cases for waku, where it reports
        // to be connected to peers but does not seem to be able to send messages
        // to them
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

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
        Ok(())
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

impl<B: NetworkBackend> ServiceState for NetworkState<B> {
    type Settings = NetworkConfig<B>;
    type Error = <B::State as ServiceState>::Error;

    fn from_settings(settings: &Self::Settings) -> Result<Self, Self::Error> {
        B::State::from_settings(&settings.backend).map(|_backend| Self { _backend })
    }
}
