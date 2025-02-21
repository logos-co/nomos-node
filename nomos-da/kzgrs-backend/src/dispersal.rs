// std
use std::hash::{Hash, Hasher};
// crates
use nomos_core::da::{
    blob::{self, metadata::Next},
    BlobId,
};
use serde::{Deserialize, Serialize};
// internal

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct BlobInfo {
    id: BlobId,
    metadata: Metadata,
}

impl BlobInfo {
    pub fn new(id: BlobId, metadata: Metadata) -> Self {
        Self { id, metadata }
    }
}

impl blob::info::DispersedBlobInfo for BlobInfo {
    type BlobId = BlobId;

    fn blob_id(&self) -> Self::BlobId {
        self.id
    }

    fn size(&self) -> usize {
        std::mem::size_of_val(&self.id) + self.metadata.size()
    }
}

impl Hash for BlobInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(<BlobInfo as blob::info::DispersedBlobInfo>::blob_id(self).as_ref());
    }
}

impl blob::metadata::Metadata for BlobInfo {
    type AppId = [u8; 32];
    type Index = Index;

    fn metadata(&self) -> (Self::AppId, Self::Index) {
        (self.metadata.app_id, self.metadata.index)
    }
}

#[derive(Copy, Clone, Default, Debug, PartialEq, PartialOrd, Ord, Eq, Serialize, Deserialize)]
pub struct Index([u8; 8]);

impl Index {
    pub fn to_u64(self) -> u64 {
        u64::from_be_bytes(self.0)
    }
}

#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Metadata {
    app_id: [u8; 32],
    index: Index,
}

impl Metadata {
    pub fn new(app_id: [u8; 32], index: Index) -> Self {
        Self { app_id, index }
    }

    pub fn size(&self) -> usize {
        std::mem::size_of_val(&self.app_id) + std::mem::size_of_val(&self.index)
    }
}

impl blob::metadata::Metadata for Metadata {
    type AppId = [u8; 32];
    type Index = Index;

    fn metadata(&self) -> (Self::AppId, Self::Index) {
        (self.app_id, self.index)
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

#[cfg(test)]
mod tests {
    use nomos_core::da::DaEncoder as _;

    use crate::{
        common::blob::DaBlob,
        encoder::{
            test::{rand_data, ENCODER},
            EncodedData,
        },
        global::GLOBAL_PARAMETERS,
        verifier::DaVerifier,
    };

    fn attest_encoded_data(encoded_data: &EncodedData, verifiers: &[DaVerifier]) -> Vec<bool> {
        let mut attestations = Vec::new();
        let domain_size = encoded_data.extended_data.0[0].len();
        for (i, column) in encoded_data.extended_data.columns().enumerate() {
            let verifier = &verifiers[i];
            let da_blob = DaBlob {
                column,
                column_idx: i
                    .try_into()
                    .expect("Column index shouldn't overflow the target type"),
                column_commitment: encoded_data.column_commitments[i],
                aggregated_column_commitment: encoded_data.aggregated_column_commitment,
                aggregated_column_proof: encoded_data.aggregated_column_proofs[i],
                rows_commitments: encoded_data.row_commitments.clone(),
                rows_proofs: encoded_data
                    .rows_proofs
                    .iter()
                    .map(|proofs| proofs.get(i).cloned().unwrap())
                    .collect(),
            };
            attestations.push(verifier.verify(&da_blob, domain_size));
        }
        attestations
    }

    #[test]
    fn test_encoded_data_verification() {
        let encoder = &ENCODER;
        let data = rand_data(8);
        let verifiers: Vec<DaVerifier> = (0..16)
            .map(|_| DaVerifier::new(GLOBAL_PARAMETERS.clone()))
            .collect();

        let encoded_data = encoder.encode(&data).unwrap();

        let attestations = attest_encoded_data(&encoded_data, &verifiers);

        assert!(!attestations.contains(&false));
    }
}
