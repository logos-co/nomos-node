use crate::common::Blob;
use crate::SubnetworkId;
use serde::{Deserialize, Serialize};

#[repr(C)]
#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReplicationRequest {
    pub blob: Blob,
    pub subnetwork_id: SubnetworkId,
}

impl ReplicationRequest {
    pub fn new(blob: Blob, subnetwork_id: SubnetworkId) -> Self {
        Self {
            blob,
            subnetwork_id,
        }
    }
}
