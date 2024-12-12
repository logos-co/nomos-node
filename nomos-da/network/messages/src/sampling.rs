use kzgrs_backend::common::ColumnIndex;
use nomos_core::da::BlobId;
use serde::{Deserialize, Serialize};

#[repr(C)]
#[derive(Serialize, Deserialize)]
pub enum SampleErrorType {
    NotFound,
}

#[repr(C)]
#[derive(Serialize, Deserialize)]
pub struct SampleError {
    pub blob_id: BlobId,
    pub error_type: SampleErrorType,
    pub error_description: String,
}

impl SampleError {
    pub fn new(
        blob_id: BlobId,
        error_type: SampleErrorType,
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
#[derive(Serialize, Deserialize)]
pub struct SampleRequest {
    pub blob_id: BlobId,
    pub column_idx: ColumnIndex,
}

impl SampleRequest {
    pub fn new(blob_id: BlobId, column_idx: ColumnIndex) -> Self {
        Self {
            blob_id,
            column_idx,
        }
    }
}
