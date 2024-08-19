// std
use std::hash::{Hash, Hasher};
// crates
use nomos_core::da::blob::{self, metadata::Next};
use serde::{Deserialize, Serialize};
// internal

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct BlobInfo {
    id: [u8; 32],
    metadata: Metadata,
}

impl BlobInfo {
    pub fn new(id: [u8; 32], metadata: Metadata) -> Self {
        Self { id, metadata }
    }
}

impl blob::info::DispersedBlobInfo for BlobInfo {
    type BlobId = [u8; 32];

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
    use blst::min_sig::SecretKey;
    use nomos_core::da::DaEncoder as _;
    use rand::{thread_rng, RngCore};

    use crate::{
        common::blob::DaBlob,
        encoder::{
            test::{rand_data, ENCODER},
            EncodedData,
        },
        verifier::DaVerifier,
    };

    fn attest_encoded_data(encoded_data: &EncodedData, verifiers: &[DaVerifier]) -> Vec<bool> {
        let mut attestations = Vec::new();
        let domain_size = encoded_data.extended_data.0[0].len();
        for (i, column) in encoded_data.extended_data.columns().enumerate() {
            let verifier = &verifiers[i];
            let da_blob = DaBlob {
                column,
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
        let mut rng = thread_rng();

        let sks: Vec<SecretKey> = (0..16)
            .map(|_| {
                let mut buff = [0u8; 32];
                rng.fill_bytes(&mut buff);
                SecretKey::key_gen(&buff, &[]).unwrap()
            })
            .collect();

        let verifiers: Vec<DaVerifier> = sks
            .clone()
            .into_iter()
            .enumerate()
            .map(|(index, sk)| DaVerifier { sk, index })
            .collect();

        let encoded_data = encoder.encode(&data).unwrap();

        let attestations = attest_encoded_data(&encoded_data, &verifiers);

        assert!(!attestations.contains(&false));
    }
}
