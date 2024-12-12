use kzgrs_backend::common::blob::DaBlob;
use nomos_core::da::BlobId;
use serde::{Deserialize, Serialize};

#[repr(C)]
#[derive(Serialize, Deserialize)]
pub struct Blob {
    pub blob_id: BlobId,
    pub data: DaBlob,
}

impl Blob {
    pub fn new(blob_id: BlobId, data: DaBlob) -> Self {
        Self { blob_id, data }
    }
}

#[repr(C)]
#[derive(Serialize, Deserialize)]
pub enum CloseMessageReason {
    GracefulShutdown = 0,
    SubnetChange = 1,
    SubnetSampleFailure = 2,
}

#[repr(C)]
#[derive(Serialize, Deserialize)]
pub struct CloseMessage {
    pub reason: CloseMessageReason,
}

impl CloseMessage {
    pub fn new(reason: CloseMessageReason) -> Self {
        CloseMessage { reason }
    }
}
