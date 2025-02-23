// std
// crates
use kzgrs::Proof;
use nomos_core::da::blob;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
// internal
use super::build_blob_id;
use super::ColumnIndex;
use crate::common::Column;
use crate::common::Commitment;
use crate::common::{
    deserialize_canonical, deserialize_vec_canonical, serialize_canonical, serialize_vec_canonical,
};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaBlob {
    pub column: Column,
    pub column_idx: ColumnIndex,
    #[serde(
        serialize_with = "serialize_canonical",
        deserialize_with = "deserialize_canonical"
    )]
    pub column_commitment: Commitment,
    #[serde(
        serialize_with = "serialize_canonical",
        deserialize_with = "deserialize_canonical"
    )]
    pub aggregated_column_commitment: Commitment,
    #[serde(
        serialize_with = "serialize_canonical",
        deserialize_with = "deserialize_canonical"
    )]
    pub aggregated_column_proof: Proof,
    #[serde(
        serialize_with = "serialize_vec_canonical",
        deserialize_with = "deserialize_vec_canonical"
    )]
    pub rows_commitments: Vec<Commitment>,
    #[serde(
        serialize_with = "serialize_vec_canonical",
        deserialize_with = "deserialize_vec_canonical"
    )]
    pub rows_proofs: Vec<Proof>,
}

impl DaBlob {
    pub fn id(&self) -> Vec<u8> {
        build_blob_id(&self.aggregated_column_commitment, &self.rows_commitments).into()
    }

    pub fn column_id(&self) -> Vec<u8> {
        let mut hasher = Sha3_256::new();
        hasher.update(self.column.as_bytes());
        hasher.finalize().as_slice().to_vec()
    }

    pub fn column_len(&self) -> usize {
        self.column.as_bytes().len()
    }

    pub fn idx(&self) -> [u8; 34] {
        create_blob_idx(&self.id(), &self.column_idx.to_be_bytes())
    }
}

impl blob::Blob for DaBlob {
    type BlobId = [u8; 32];
    type ColumnIndex = [u8; 2];
    type LightBlob = DaLightBlob;
    type SharedCommitments = DaBlobSharedCommitments;

    fn id(&self) -> Self::BlobId {
        build_blob_id(&self.aggregated_column_commitment, &self.rows_commitments)
    }

    fn column_idx(&self) -> Self::ColumnIndex {
        self.column_idx.to_be_bytes()
    }

    fn into_blob_and_shared_commitments(self) -> (Self::LightBlob, Self::SharedCommitments) {
        (
            DaLightBlob {
                column_idx: self.column_idx,
                column: self.column,
                column_commitment: self.column_commitment,
                aggregated_column_proof: self.aggregated_column_proof,
                rows_proofs: self.rows_proofs,
            },
            DaBlobSharedCommitments {
                aggregated_column_commitment: self.aggregated_column_commitment,
                rows_commitments: self.rows_commitments,
            },
        )
    }

    fn from_blob_and_shared_commitments(
        light_blob: Self::LightBlob,
        shared_commitments: Self::SharedCommitments,
    ) -> Self {
        Self {
            column: light_blob.column,
            column_idx: light_blob.column_idx,
            column_commitment: light_blob.column_commitment,
            aggregated_column_commitment: shared_commitments.aggregated_column_commitment,
            aggregated_column_proof: light_blob.aggregated_column_proof,
            rows_commitments: shared_commitments.rows_commitments,
            rows_proofs: light_blob.rows_proofs,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaLightBlob {
    pub column_idx: ColumnIndex,
    pub column: Column,
    #[serde(
        serialize_with = "serialize_canonical",
        deserialize_with = "deserialize_canonical"
    )]
    pub column_commitment: Commitment,
    #[serde(
        serialize_with = "serialize_canonical",
        deserialize_with = "deserialize_canonical"
    )]
    pub aggregated_column_proof: Proof,
    #[serde(
        serialize_with = "serialize_vec_canonical",
        deserialize_with = "deserialize_vec_canonical"
    )]
    pub rows_proofs: Vec<Proof>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaBlobSharedCommitments {
    #[serde(
        serialize_with = "serialize_canonical",
        deserialize_with = "deserialize_canonical"
    )]
    pub aggregated_column_commitment: Commitment,
    #[serde(
        serialize_with = "serialize_vec_canonical",
        deserialize_with = "deserialize_vec_canonical"
    )]
    pub rows_commitments: Vec<Commitment>,
}

// Combines a 32-byte blob ID (`[u8; 32]`) with a 2-byte column index
// (`u16` represented as `[u8; 2]`).
pub fn create_blob_idx(blob_id: &[u8], column_idx: &[u8]) -> [u8; 34] {
    let mut blob_idx = [0u8; 34];
    blob_idx[..blob_id.len()].copy_from_slice(blob_id);
    blob_idx[blob_id.len()..].copy_from_slice(column_idx);

    blob_idx
}
