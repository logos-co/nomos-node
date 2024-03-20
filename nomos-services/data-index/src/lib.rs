use std::fmt::{Debug, Formatter};

use serde::{Deserialize, Serialize};

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

pub enum DaIndexMsg<C: Certificate> {
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

pub struct DataIndexerService<Protocol>
where
    Protocol: DaProtocol,
    Protocol::Certificate: 'static,
{
    service_state: ServiceStateHandle<Self>,
    da: Protocol,
}

impl<P> ServiceData for DataIndexerService<P>
where
    P: DaProtocol,
    P::Certificate: 'static,
{
    const SERVICE_ID: ServiceId = "DAIndexer";
    type Settings = Settings<P::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = DaIndexMsg<P::Certificate>;
}

#[async_trait::async_trait]
impl<P> ServiceCore for DataIndexerService<P>
where
    P: DaProtocol + Send + Sync,
    P::Certificate: Send + 'static,
    P::Settings: Clone + Send + Sync + 'static,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let settings = service_state.settings_reader.get_updated_settings();
        let da = P::new(settings.da_protocol);
        Ok(Self { service_state, da })
    }

    async fn run(self) -> Result<(), DynError> {
        todo!()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings<P> {
    pub da_protocol: P,
}
