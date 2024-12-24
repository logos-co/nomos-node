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
}

impl blob::Blob for DaBlob {
    type BlobId = [u8; 32];
    type ColumnIndex = [u8; 2];

    fn id(&self) -> Self::BlobId {
        build_blob_id(&self.aggregated_column_commitment, &self.rows_commitments)
    }

    fn column_idx(&self) -> Self::ColumnIndex {
        self.column_idx.to_be_bytes()
    }
}

#[cfg(test)]
mod tests {
    use crate::common::blob::DaBlob;
    use crate::common::{deserialize_canonical, serialize_canonical};
    use crate::common::{Chunk, Column};
    use crate::encoder::{DaEncoder, DaEncoderParams};
    use crate::global::GLOBAL_PARAMETERS;
    use kzgrs::{Commitment, Proof};
    use nomos_core::da::DaEncoder as _;
    use rand::{thread_rng, RngCore};
    use serde::{Deserialize, Serialize};

    pub fn get_da_blob(data: Vec<u8>) -> DaBlob {
        let encoder_params = DaEncoderParams::new(2048, true, GLOBAL_PARAMETERS.clone());
        let encoder = DaEncoder::new(encoder_params);
        let encoded_data = encoder.encode(&data).unwrap();
        let columns: Vec<_> = encoded_data.extended_data.columns().collect();

        let index = 0;
        let da_blob = DaBlob {
            column: columns[index].clone(),
            column_idx: index
                .try_into()
                .expect("Column index shouldn't overflow the target type"),
            column_commitment: encoded_data.column_commitments[index],
            aggregated_column_commitment: encoded_data.aggregated_column_commitment,
            aggregated_column_proof: encoded_data.aggregated_column_proofs[index],
            rows_commitments: encoded_data.row_commitments.clone(),
            rows_proofs: encoded_data
                .rows_proofs
                .iter()
                .map(|proofs| proofs.get(index).cloned().unwrap())
                .collect(),
        };

        da_blob
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct DaChunk {
        chunk: Chunk,
        #[serde(
            serialize_with = "serialize_canonical",
            deserialize_with = "deserialize_canonical"
        )]
        chunk_proof: Proof,
        column_idx: usize,
        #[serde(
            serialize_with = "serialize_canonical",
            deserialize_with = "deserialize_canonical"
        )]
        column_commitment: Commitment,
        #[serde(
            serialize_with = "serialize_canonical",
            deserialize_with = "deserialize_canonical"
        )]
        aggregated_column_commitment: Commitment,
        #[serde(
            serialize_with = "serialize_canonical",
            deserialize_with = "deserialize_canonical"
        )]
        row_commitment: Commitment,
        #[serde(
            serialize_with = "serialize_canonical",
            deserialize_with = "deserialize_canonical"
        )]
        row_proofs: Proof,
    }

    fn da_blob_to_single_chunk(blob: &DaBlob) -> DaChunk {
        DaChunk {
            chunk: blob.column.0[0].clone(),
            chunk_proof: blob.rows_proofs[0].clone(),
            column_idx: 0,
            column_commitment: blob.column_commitment.clone(),
            aggregated_column_commitment: blob.aggregated_column_commitment.clone(),
            row_commitment: blob.rows_commitments[0].clone(),
            row_proofs: blob.rows_proofs[0].clone(),
        }
    }

    // This test is just for information purposes it doesn't actually test anything. Simply ignore
    #[ignore]
    #[test]
    fn test_sizes() {
        let sizes: &[usize] = &[
            4224,  // 128Kb / 31
            8456,  // 256Kb / 31
            16912, // 512Kb / 31
            33825, // 1024Kb / 31
        ];
        for size in sizes {
            let mut data = vec![0u8; 31 * size];
            thread_rng().fill_bytes(&mut data);
            println!("Data size: {}bytes", data.len());
            let blob = get_da_blob(data);
            let chunk = da_blob_to_single_chunk(&blob);
            println!("Column len: {}items", blob.column.len());
            println!("Column size: {}bytes", blob.column.as_bytes().len());
            let encoded = bincode::serialize(&blob).unwrap();
            println!(
                "Column:\n\tsample size: {}bytes, {}Kb",
                encoded.len(),
                encoded.len() / 1024
            );
            let encoded = bincode::serialize(&chunk).unwrap();
            println!(
                "Chunk:\n\tsample size: {}bytes, {}Kb",
                encoded.len(),
                encoded.len() / 1024
            );
        }
    }
}
