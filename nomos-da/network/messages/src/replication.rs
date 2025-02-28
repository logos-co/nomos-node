use serde::{Deserialize, Serialize};

use crate::{common::Blob, SubnetworkId};

#[repr(C)]
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReplicationRequest {
    pub blob: Blob,
    pub subnetwork_id: SubnetworkId,
}

impl ReplicationRequest {
    #[must_use]
    pub const fn new(blob: Blob, subnetwork_id: SubnetworkId) -> Self {
        Self {
            blob,
            subnetwork_id,
        }
    }
}
