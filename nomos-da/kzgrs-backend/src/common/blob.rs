// std
use std::io::Cursor;
// crates
use ark_serialize::*;
use kzgrs::Proof;
use nomos_core::da::blob;
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha3::{Digest, Sha3_256};
// internal
use super::build_attestation_message;
use crate::common::Column;
use crate::common::Commitment;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaBlob {
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
        build_attestation_message(&self.aggregated_column_commitment, &self.rows_commitments)
    }

    pub fn column_id(&self) -> Vec<u8> {
        let mut hasher = Sha3_256::new();
        hasher.update(self.column.as_bytes());
        hasher.finalize().as_slice().to_vec()
    }
}

impl blob::Blob for DaBlob {
    type BlobId = Vec<u8>;

    fn id(&self) -> Self::BlobId {
        build_attestation_message(&self.aggregated_column_commitment, &self.rows_commitments)
    }
}

fn serialize_canonical<S, T>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: CanonicalSerialize,
{
    let mut bytes = Vec::new();
    value
        .serialize_compressed(&mut bytes)
        .map_err(serde::ser::Error::custom)?;
    serializer.serialize_bytes(&bytes)
}

fn deserialize_canonical<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: CanonicalDeserialize,
{
    let bytes: Vec<u8> = serde::Deserialize::deserialize(deserializer)?;
    let mut cursor = Cursor::new(bytes);
    T::deserialize_compressed(&mut cursor).map_err(serde::de::Error::custom)
}

fn serialize_vec_canonical<S, T>(values: &[T], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: CanonicalSerialize,
{
    let mut container = serializer.serialize_seq(Some(values.len()))?;
    for value in values {
        let mut bytes = Vec::new();
        value
            .serialize_compressed(&mut bytes)
            .map_err(serde::ser::Error::custom)?;
        container.serialize_element(&bytes)?;
    }
    container.end()
}

fn deserialize_vec_canonical<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: CanonicalDeserialize,
{
    let bytes_vecs: Vec<Vec<u8>> = Deserialize::deserialize(deserializer)?;
    bytes_vecs
        .iter()
        .map(|bytes| {
            let mut cursor = Cursor::new(bytes);
            T::deserialize_compressed(&mut cursor).map_err(serde::de::Error::custom)
        })
        .collect()
}
