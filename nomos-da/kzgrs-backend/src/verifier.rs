use crate::common::{build_attestation_message, Column};
use kzgrs::{Commitment, Proof};
use sha3::{Digest, Sha3_256};

pub struct DaBlob {
    column: Column,
    column_commitment: Commitment,
    aggregated_column_commitment: Commitment,
    aggregated_column_proof: Proof,
    rows_commitments: Vec<Commitment>,
    rows_proofs: Vec<Proof>,
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
