// crates
use async_trait::async_trait;
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::RelayMessage,
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};

pub struct RobustnessService {}

impl ServiceData for RobustnessService {
    const SERVICE_ID: ServiceId = "Robustness";
    type Settings = RobustnessConfig;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = RobustnessMsg;
}

#[derive(Clone)]
pub struct RobustnessConfig {}

#[derive(Debug)]
pub enum RobustnessMsg {
    /// Inject a new entropy that will be used for configuration update logic
    SetEntropy(Box<u8>),
}

impl RelayMessage for RobustnessMsg {}

#[async_trait]
impl ServiceCore for RobustnessService {
    fn init(_: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        todo!()
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
        todo!()
    }
}
