// std

// crates

use overwatch_rs::services::handle::{ServiceHandle, ServiceStateHandle};
use overwatch_rs::DynError;
use std::marker::PhantomData;
// internal
use overwatch_rs::services::relay::RelayMessage;
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use tokio::sync::oneshot;

pub mod backend;

const DA_DISPERSAL_TAG: ServiceId = "DA-Encoder";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Foo bar {0}")]
    Encoding(String),
}

#[derive(Debug)]
pub enum DaDispersalMsg {
    Disperse {
        blob: Vec<u8>,
        reply_channel: oneshot::Sender<Result<(), Error>>,
    },
}

impl RelayMessage for DaDispersalMsg {}

pub struct DispersalService<Backend> {
    service_handle: ServiceHandle<Self>,
    _backend: PhantomData<Backend>,
}

impl<B> ServiceData for DispersalService<B> {
    const SERVICE_ID: ServiceId = DA_DISPERSAL_TAG;
    type Settings = ();
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = DaDispersalMsg;
}

#[async_trait::async_trait]
impl<Backend> ServiceCore for DispersalService<Backend>
where
    Backend: Send,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        todo!()
    }

    async fn run(self) -> Result<(), DynError> {
        todo!()
    }
}
