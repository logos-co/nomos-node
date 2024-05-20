// std
// crates
use ark_serialize::*;
use kzgrs::Proof;
use nomos_core::da::blob;
use serde::de::{self, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha3::{Digest, Sha3_256};
// internal
use super::build_attestation_message;
use crate::common::Column;
use crate::common::Commitment;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaBlob {
    pub(crate) column: Column,
    #[serde(
        serialize_with = "serialize_commitment",
        deserialize_with = "deserialize_commitment"
    )]
    pub(crate) column_commitment: Commitment,
    #[serde(
        serialize_with = "serialize_commitment",
        deserialize_with = "deserialize_commitment"
    )]
    pub(crate) aggregated_column_commitment: Commitment,
    #[serde(
        serialize_with = "serialize_proof",
        deserialize_with = "deserialize_proof"
    )]
    pub(crate) aggregated_column_proof: Proof,
    #[serde(
        serialize_with = "serialize_vec_commitment",
        deserialize_with = "deserialize_vec_commitment"
    )]
    pub(crate) rows_commitments: Vec<Commitment>,
    #[serde(
        serialize_with = "serialize_vec_proof",
        deserialize_with = "deserialize_vec_proof"
    )]
    pub(crate) rows_proofs: Vec<Proof>,
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

fn serialize_commitment<S>(commitment: &Commitment, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut bytes = Vec::new();
    commitment
        .serialize_with_mode(&mut bytes, Compress::Yes)
        .map_err(serde::ser::Error::custom)?;
    serializer.serialize_bytes(&bytes)
}

fn deserialize_commitment<'de, D>(deserializer: D) -> Result<Commitment, D::Error>
where
    D: Deserializer<'de>,
{
    struct BytesVisitor;

    impl<'de> Visitor<'de> for BytesVisitor {
        type Value = Commitment;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a byte array representing a Commitment")
        }

        fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let cursor = std::io::Cursor::new(v);
            Commitment::deserialize_with_mode(cursor, Compress::Yes, Validate::Yes)
                .map_err(de::Error::custom)
        }
    }

    deserializer.deserialize_bytes(BytesVisitor)
}

fn serialize_vec_commitment<S>(commitments: &[Commitment], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut container = serializer.serialize_seq(Some(commitments.len()))?;
    for commitment in commitments {
        let mut bytes = Vec::new();
        commitment
            .serialize_with_mode(&mut bytes, ark_serialize::Compress::Yes)
            .map_err(serde::ser::Error::custom)?;
        container.serialize_element(&bytes)?;
    }
    container.end()
}

fn deserialize_vec_commitment<'de, D>(deserializer: D) -> Result<Vec<Commitment>, D::Error>
where
    D: Deserializer<'de>,
{
    struct VecBytesVisitor;

    impl<'de> Visitor<'de> for VecBytesVisitor {
        type Value = Vec<Commitment>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a sequence of byte arrays representing Commitments")
        }

        fn visit_seq<V>(self, mut seq: V) -> Result<Vec<Commitment>, V::Error>
        where
            V: SeqAccess<'de>,
        {
            let mut commitments = Vec::new();
            while let Some(bytes) = seq.next_element::<Vec<u8>>()? {
                let cursor = std::io::Cursor::new(bytes);
                let commitment = Commitment::deserialize_with_mode(
                    cursor,
                    ark_serialize::Compress::Yes,
                    ark_serialize::Validate::Yes,
                )
                .map_err(de::Error::custom)?;
                commitments.push(commitment);
            }
            Ok(commitments)
        }
    }

    deserializer.deserialize_seq(VecBytesVisitor)
}

fn serialize_vec_proof<S>(proofs: &[Proof], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut container = serializer.serialize_seq(Some(proofs.len()))?;
    for proof in proofs {
        let mut bytes = Vec::new();
        proof
            .serialize_with_mode(&mut bytes, ark_serialize::Compress::Yes)
            .map_err(serde::ser::Error::custom)?;
        container.serialize_element(&bytes)?;
    }
    container.end()
}

fn deserialize_vec_proof<'de, D>(deserializer: D) -> Result<Vec<Proof>, D::Error>
where
    D: Deserializer<'de>,
{
    struct VecBytesVisitor;

    impl<'de> Visitor<'de> for VecBytesVisitor {
        type Value = Vec<Proof>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a sequence of byte arrays representing Proofs")
        }

        fn visit_seq<V>(self, mut seq: V) -> Result<Vec<Proof>, V::Error>
        where
            V: SeqAccess<'de>,
        {
            let mut proofs = Vec::new();
            while let Some(bytes) = seq.next_element::<Vec<u8>>()? {
                let cursor = std::io::Cursor::new(bytes);
                let proof = Proof::deserialize_with_mode(
                    cursor,
                    ark_serialize::Compress::Yes,
                    ark_serialize::Validate::Yes,
                )
                .map_err(de::Error::custom)?;
                proofs.push(proof);
            }
            Ok(proofs)
        }
    }

    deserializer.deserialize_seq(VecBytesVisitor)
}

fn serialize_proof<S>(proof: &Proof, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut bytes = Vec::new();
    proof
        .serialize_with_mode(&mut bytes, Compress::Yes)
        .map_err(serde::ser::Error::custom)?;
    serializer.serialize_bytes(&bytes)
}

fn deserialize_proof<'de, D>(deserializer: D) -> Result<Proof, D::Error>
where
    D: Deserializer<'de>,
{
    struct BytesVisitor;

    impl<'de> Visitor<'de> for BytesVisitor {
        type Value = Proof;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a byte array representing a Proof")
        }

        fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let cursor = std::io::Cursor::new(v);
            Proof::deserialize_with_mode(cursor, Compress::Yes, Validate::Yes)
                .map_err(de::Error::custom)
        }
    }

    deserializer.deserialize_bytes(BytesVisitor)
}
