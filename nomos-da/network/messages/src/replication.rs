use serde::{Deserialize, Serialize};

use crate::{common::Share, SubnetworkId};

#[repr(C)]
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReplicationRequest {
    pub share: Share,
    pub subnetwork_id: SubnetworkId,
}

impl ReplicationRequest {
    #[must_use]
    pub const fn new(share: Share, subnetwork_id: SubnetworkId) -> Self {
        Self {
            share,
            subnetwork_id,
        }
    }
}
