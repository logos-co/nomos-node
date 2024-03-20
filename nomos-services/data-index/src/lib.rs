pub mod backend;

// std
use std::fmt::{Debug, Formatter};
// crates
use serde::{Deserialize, Serialize};
use futures::StreamExt;
use tracing::error;
// internal
use crate::backend::{DaIndexBackend, DaIndexError};
use nomos_core::da::{certificate::Certificate, DaProtocol};
use overwatch_rs::{
    services::{
        handle::ServiceStateHandle,
        relay::RelayMessage,
        state::{NoOperator, NoState},
        ServiceCore, ServiceData, ServiceId,
    },
    DynError,
};
use overwatch_rs::services::life_cycle::LifecycleMessage;

pub enum DaIndexMsg<C: Certificate> {
    // TODO: The certificate receival most likely won't happen by sending a new VIDs
    // to the DAIndexService. The service should subscribe to the event about new VIDs
    // observerd/added to the block.
    ReceiveCert { cert: C },
    GetRange {},
}

impl<C: Certificate + 'static> Debug for DaIndexMsg<C> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DaIndexMsg::ReceiveCert { .. } => {
                write!(f, "DaIndexMsg::ReceiveVID")
            }
            DaIndexMsg::GetRange { .. } => {
                write!(f, "DaIndexMsg::Get")
            }
        }
    }
}

impl<C: Certificate + 'static> RelayMessage for DaIndexMsg<C> {}

pub struct DataIndexerService<Protocol, Backend>
where
    Protocol: DaProtocol,
    Protocol::Certificate: 'static,
    Backend: DaIndexBackend,
{
    service_state: ServiceStateHandle<Self>,
    da: Protocol,
    backend: Backend,
}

impl<P, B> DataIndexerService<P, B> 
where
    P: DaProtocol + Send + Sync,
    P::Certificate: Send + 'static,
    B: DaIndexBackend + Send + Sync 
{
    async fn handle_inbound_msg(backend: &B, msg: DaIndexMsg<B::Certificate>) -> Result<(), DaIndexError> {
        todo!()
    }

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

impl<P, B> ServiceData for DataIndexerService<P, B>
where
    P: DaProtocol,
    P::Certificate: 'static,
    B: DaIndexBackend,
{
    const SERVICE_ID: ServiceId = "DAIndexer";
    type Settings = Settings<P::Settings, B::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = DaIndexMsg<P::Certificate>;
}

#[async_trait::async_trait]
impl<P, B> ServiceCore for DataIndexerService<P, B>
where
    P: DaProtocol + Send + Sync,
    P::Certificate: Send + 'static,
    P::Settings: Clone + Send + Sync + 'static,
    B: DaIndexBackend + Send + Sync,
    B: DaIndexBackend<Certificate = P::Certificate> + Send + Sync,
    B::Settings: Clone + Send + Sync + 'static,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let settings = service_state.settings_reader.get_updated_settings();
        let da = P::new(settings.da_protocol);
        let backend = B::new(settings.backend);
        Ok(Self { service_state, da, backend})
    }

    async fn run(self) -> Result<(), DynError> {
        let Self {
            mut service_state,
            da,
            backend,
        } = self;
        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();

        loop {
            tokio::select! {
                Some(msg) = service_state.inbound_relay.recv() => {
                    if let Err(e) = Self::handle_inbound_msg(&backend, msg).await {
                        tracing::debug!("Failed to handle da msg: {e:?}");
                    }
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings<P, B> {
    pub da_protocol: P,
    pub backend: B,
}
