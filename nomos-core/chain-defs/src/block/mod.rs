pub mod builder;

use core::hash::Hash;

use ::serde::{de::DeserializeOwned, Deserialize, Serialize};
use bytes::Bytes;
use indexmap::IndexSet;

use crate::{header::Header, wire};

pub type TxHash = [u8; 32];

/// A block
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block<Tx: Clone + Eq + Hash, BlobCertificate: Clone + Eq + Hash> {
    header: Header,
    cl_transactions: IndexSet<Tx>,
    bl_blobs: IndexSet<BlobCertificate>,
}

impl<Tx: Clone + Eq + Hash, BlobCertificate: Clone + Eq + Hash> Block<Tx, BlobCertificate> {
    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn transactions(&self) -> impl Iterator<Item = &Tx> + '_ {
        self.cl_transactions.iter()
    }

    pub fn blobs(&self) -> impl Iterator<Item = &BlobCertificate> + '_ {
        self.bl_blobs.iter()
    }
}

impl<
        Tx: Clone + Eq + Hash + Serialize + DeserializeOwned,
        BlobCertificate: Clone + Eq + Hash + Serialize + DeserializeOwned,
    > Block<Tx, BlobCertificate>
{
    /// Encode block into bytes
    pub fn as_bytes(&self) -> Bytes {
        wire::serialize(self).unwrap().into()
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        wire::deserialize(bytes).unwrap()
    }

    pub fn cl_transactions_len(&self) -> usize {
        self.cl_transactions.len()
    }

    pub fn bl_blobs_len(&self) -> usize {
        self.bl_blobs.len()
    }
}
