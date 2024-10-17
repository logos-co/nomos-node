pub mod backends;

use std::{
    fmt::{self, Debug},
    pin::Pin,
};

use async_trait::async_trait;
use backends::MixBackend;
use futures::{Stream, StreamExt};
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    life_cycle::LifecycleMessage,
    relay::RelayMessage,
    state::{NoOperator, ServiceState},
    ServiceCore, ServiceData, ServiceId,
};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

pub struct MixService<B: MixBackend + 'static> {
    backend: B,
    service_state: ServiceStateHandle<Self>,
}

impl<B: MixBackend + 'static> ServiceData for MixService<B> {
    const SERVICE_ID: ServiceId = "Mix";
    type Settings = MixConfig<B>;
    type State = NetworkState<B>;
    type StateOperator = NoOperator<Self::State>;
    type Message = MixServiceMsg<B>;
}

#[async_trait]
impl<B> ServiceCore for MixService<B>
where
    B: MixBackend + Send + 'static,
    B::State: Send + Sync,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        Ok(Self {
            backend: <B as MixBackend>::new(
                service_state.settings_reader.get_updated_settings().backend,
                service_state.overwatch_handle.clone(),
            ),
            service_state,
        })
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
        let Self {
            service_state:
                ServiceStateHandle {
                    mut inbound_relay,
                    lifecycle_handle,
                    ..
                },
            mut backend,
        } = self;
        let mut lifecycle_stream = lifecycle_handle.message_stream();
        loop {
            tokio::select! {
                Some(msg) = inbound_relay.recv() => {
                    Self::handle_service_message(msg, &mut backend).await;
                }
                Some(msg) = lifecycle_stream.next() => {
                    if Self::should_stop_service(msg).await {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}

impl<B> MixService<B>
where
    B: MixBackend + Send + 'static,
    B::State: Send + Sync,
{
    async fn handle_service_message(msg: MixServiceMsg<B>, backend: &mut B) {
        match msg {
            MixServiceMsg::Process(msg) => {
                // split sending in two steps to help the compiler understand we do not
                // need to hold an instance of &I (which is not send) across an await point
                let _send = backend.process(msg);
                _send.await
            }
            MixServiceMsg::Subscribe { kind, sender } => sender
                .send(backend.subscribe(kind).await)
                .unwrap_or_else(|_| {
                    tracing::warn!(
                        "client hung up before a subscription handle could be established"
                    )
                }),
        }
    }

    async fn should_stop_service(msg: LifecycleMessage) -> bool {
        match msg {
            LifecycleMessage::Kill => true,
            LifecycleMessage::Shutdown(signal_sender) => {
                // TODO: Maybe add a call to backend to handle this. Maybe trying to save unprocessed messages?
                if signal_sender.send(()).is_err() {
                    tracing::error!(
                        "Error sending successful shutdown signal from service {}",
                        Self::SERVICE_ID
                    );
                }
                true
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct MixConfig<B: MixBackend> {
    pub backend: B::Settings,
}

impl<B: MixBackend> Debug for MixConfig<B> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "MixConfig {{ backend: {:?}}}", self.backend)
    }
}

impl<B: MixBackend> Clone for MixConfig<B> {
    fn clone(&self) -> Self {
        MixConfig {
            backend: self.backend.clone(),
        }
    }
}

pub struct NetworkState<B: MixBackend> {
    _backend: B::State,
}

impl<B: MixBackend> Clone for NetworkState<B> {
    fn clone(&self) -> Self {
        NetworkState {
            _backend: self._backend.clone(),
        }
    }
}

impl<B: MixBackend> ServiceState for NetworkState<B> {
    type Settings = MixConfig<B>;
    type Error = <B::State as ServiceState>::Error;

    fn from_settings(settings: &Self::Settings) -> Result<Self, Self::Error> {
        B::State::from_settings(&settings.backend).map(|_backend| Self { _backend })
    }
}

pub enum MixServiceMsg<B: MixBackend> {
    Process(B::Message),
    Subscribe {
        kind: B::EventKind,
        sender: oneshot::Sender<Pin<Box<dyn Stream<Item = B::NetworkEvent> + Send>>>,
    },
}

impl<B: MixBackend> Debug for MixServiceMsg<B> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Process(msg) => write!(fmt, "NetworkMsg::Process({msg:?})"),
            Self::Subscribe { kind, .. } => write!(fmt, "NetworkMsg::Subscribe{{ kind: {kind:?}}}"),
        }
    }
}

impl<T: MixBackend + 'static> RelayMessage for MixServiceMsg<T> {}
