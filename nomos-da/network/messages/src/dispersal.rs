use crate::common::Blob;
use crate::SubnetworkId;
use nomos_core::da::BlobId;
use serde::{Deserialize, Serialize};

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DispersalErrorType {
    ChunkSize,
    Verification,
}

#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DispersalError {
    pub blob_id: BlobId,
    pub error_type: DispersalErrorType,
    pub error_description: String,
}

impl DispersalError {
    pub fn new(
        blob_id: BlobId,
        error_type: DispersalErrorType,
        error_description: impl Into<String>,
    ) -> Self {
        Self {
            blob_id,
            error_type,
            error_description: error_description.into(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DispersalRequest {
    pub blob: Blob,
    pub subnetwork_id: SubnetworkId,
}

impl DispersalRequest {
    pub fn new(blob: Blob, subnetwork_id: SubnetworkId) -> Self {
        Self {
            blob,
            subnetwork_id,
        }
    }
}

#[repr(C)]
#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DispersalResponse {
    BlobId(BlobId),
    Error(DispersalError),
}
