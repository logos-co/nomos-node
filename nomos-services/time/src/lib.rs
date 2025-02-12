use crate::backends::TimeBackend;
use cryptarchia_engine::{Epoch, Slot};
use futures::{Stream, StreamExt};
use log::error;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::relay::RelayMessage;
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use overwatch_rs::DynError;
use std::fmt::{Debug, Formatter};
use tokio::sync::oneshot;

pub mod backends;

const TIME_SERVICE_TAG: ServiceId = "time-service";

pub struct SlotTick {
    epoch: Epoch,
    slot: Slot,
}

pub type SlotTickStream = Box<dyn Stream<Item = SlotTick> + Send>;

pub enum TimeServiceMessage {
    Subscribe {
        sender: oneshot::Sender<SlotTickStream>,
    },
}

impl Debug for TimeServiceMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TimeServiceMessage::Subscribe { .. } => f.write_str("New time service subscription"),
        }
    }
}

impl RelayMessage for TimeServiceMessage {}

#[derive(Clone)]
pub struct TimeServiceSettings<BackendSettings> {
    backend_settings: BackendSettings,
}

pub struct TimeService<Backend>
where
    Backend: TimeBackend,
    Backend::Settings: Clone,
{
    state: ServiceStateHandle<Self>,
    backend: Backend,
}

impl<Backend> ServiceData for TimeService<Backend>
where
    Backend: TimeBackend,
    Backend::Settings: Clone,
{
    const SERVICE_ID: ServiceId = TIME_SERVICE_TAG;
    type Settings = TimeServiceSettings<Backend::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = TimeServiceMessage;
}

#[async_trait::async_trait]
impl<Backend> ServiceCore for TimeService<Backend>
where
    Backend: TimeBackend + Send,
    Backend::Settings: Clone + Send + Sync,
{
    fn init(
        service_state: ServiceStateHandle<Self>,
        _initial_state: Self::State,
    ) -> Result<Self, DynError> {
        let Self::Settings {
            backend_settings, ..
        } = service_state.settings_reader.get_updated_settings();
        let backend = Backend::init(backend_settings, service_state.overwatch_handle.clone());
        Ok(Self {
            state: service_state,
            backend,
        })
    }

    async fn run(self) -> Result<(), DynError> {
        let Self { state, backend } = self;
        let mut inbound_relay = state.inbound_relay;
        let mut lifecycle_relay = state.lifecycle_handle.message_stream();
        loop {
            tokio::select! {
                Some(service_message) = inbound_relay.recv() => {
                    match service_message {
                        TimeServiceMessage::Subscribe { sender} => {
                            let stream =
                            if let Err(e) = sender.send(backend.tick_stream()) {
                                error!("Error subscribing to time event: Couldn't send back a response");
                            };
                        }
                    }
                }
                Some(lifecycle_msg) = lifecycle_relay.next() => {
                    // TODO: Add handling after refactor duplicated handling function in other services.
                    todo!("Handle lifecyle");
                }
            }
        }
        Ok(())
    }
}
