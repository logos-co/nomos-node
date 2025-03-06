use std::hash::{Hash, Hasher};

use bytes::Bytes;
use nomos_core::da::blob::{
    info::DispersedBlobInfo,
    metadata::{self, Next},
};
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Default, Debug, PartialEq, PartialOrd, Ord, Eq, Serialize, Deserialize)]
pub struct Index([u8; 8]);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub voter: Voter,
    pub num_attestations: usize,
}

pub type Voter = [u8; 32];

#[derive(Debug, Clone, Serialize, Deserialize, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Blob {
    data: Bytes,
}

#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Metadata {
    app_id: [u8; 32],
    index: Index,
}

impl Metadata {
    #[must_use]
    pub const fn new(app_id: [u8; 32], index: Index) -> Self {
        Self { app_id, index }
    }

    #[expect(
        clippy::missing_const_for_fn,
        reason = "`std::mem::size_of_val` is not yet stable as a const fn"
    )]
    fn size(&self) -> usize {
        std::mem::size_of_val(&self.app_id) + std::mem::size_of_val(&self.index)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BlobInfo {
    id: [u8; 32],
    metadata: Metadata,
}

impl BlobInfo {
    #[must_use]
    pub const fn new(id: [u8; 32], metadata: Metadata) -> Self {
        Self { id, metadata }
    }
}

impl Hash for BlobInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(<Self as DispersedBlobInfo>::blob_id(self).as_ref());
    }
}

impl DispersedBlobInfo for BlobInfo {
    type BlobId = [u8; 32];

    fn blob_id(&self) -> Self::BlobId {
        self.id
    }

    fn size(&self) -> usize {
        std::mem::size_of_val(&self.id) + self.metadata.size()
    }
}

impl metadata::Metadata for BlobInfo {
    type AppId = [u8; 32];
    type Index = Index;

    fn metadata(&self) -> (Self::AppId, Self::Index) {
        (self.metadata.app_id, self.metadata.index)
    }
}

impl From<u64> for Index {
    fn from(value: u64) -> Self {
        Self(value.to_be_bytes())
    }
}

impl Next for Index {
    fn next(self) -> Self {
        let num = u64::from_be_bytes(self.0);
        let incremented_num = num.wrapping_add(1);
        Self(incremented_num.to_be_bytes())
    }
}

impl AsRef<[u8]> for Index {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}
