mod services;

use overwatch_derive::Services;
use overwatch_rs::services::handle::ServiceHandle;
use overwatch_rs::services::ServiceData;
use serde::{Deserialize, Serialize};
use services::mixnet::MixnetNodeService;

#[derive(Deserialize, Debug, Clone, Serialize)]
pub struct Config {
    // pub log: <Logger as ServiceData>::Settings,
    pub mixnode: <MixnetNodeService as ServiceData>::Settings,
}

#[derive(Services)]
pub struct MixNode {
    node: ServiceHandle<MixnetNodeService>,
}
