use mixnet_node::{MixnetNode, MixnetNodeConfig};
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::relay::NoMessage;
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use overwatch_rs::DynError;

pub struct MixnetNodeService(MixnetNode);

impl ServiceData for MixnetNodeService {
    const SERVICE_ID: ServiceId = "mixnet-node";
    type Settings = MixnetNodeConfig;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NoMessage;
}

#[async_trait::async_trait]
impl ServiceCore for MixnetNodeService {
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let settings: Self::Settings = service_state.settings_reader.get_updated_settings();
        Ok(Self(MixnetNode::new(settings)))
    }

    async fn run(self) -> Result<(), DynError> {
        if let Err(_e) = self.0.run().await {
            todo!("Errors should match");
        }
        Ok(())
    }
}
