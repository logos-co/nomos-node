use std::{error::Error, num::NonZeroUsize};

use crate::KzgBundle;

pub struct MockKzgSettings {
    pub rows: NonZeroUsize,
    pub cols: NonZeroUsize,
}

pub struct MockRowCommitment {}

pub struct MockColCommitment {}

pub struct MockMasterCommitment {
    pub nonce: u64,
}

#[derive(PartialEq, Eq)]
pub struct MockProof {
    pub cks: u32,
}

pub struct MockKzgBundle {
    pub master_commitment: MockMasterCommitment,
    pub row_commitments: Vec<MockRowCommitment>,
    pub col_commitments: Vec<MockColCommitment>,
    pub proof: Vec<MockProof>,
}

impl super::KzgBundle for MockKzgBundle {
    type RowCommitment = MockRowCommitment;

    type ColCommitment = MockColCommitment;

    type MasterCommitment = MockMasterCommitment;

    type Proof = MockProof;

    type Nonce = u64;

    fn master_commitment(&self) -> &Self::MasterCommitment {
        &self.master_commitment
    }

    fn row_commitments(&self) -> &[Self::RowCommitment] {
        &self.row_commitments
    }

    fn col_commitments(&self) -> &[Self::ColCommitment] {
        &self.col_commitments
    }

    fn proof(&self) -> &[Self::Proof] {
        &self.proof
    }

    fn nonce(&self) -> &Self::Nonce {
        &self.master_commitment.nonce
    }
}

pub struct MockKzg {
    pub settings: MockKzgSettings,
}

impl super::KzgProvider for MockKzg {
    type Bundle = MockKzgBundle;

    type Settings = MockKzgSettings;

    type Nonce = u64;

    fn compute_commitment(
        &self,
        data: &[u8],
        nonce: Self::Nonce,
    ) -> Result<Self::Bundle, Box<dyn std::error::Error>> {
        Ok(MockKzgBundle {
            master_commitment: MockMasterCommitment { nonce: nonce + 1 },
            row_commitments: Vec::new(),
            col_commitments: Vec::new(),
            proof: vec![MockProof {
                cks: crc32fast::hash(data),
            }],
        })
    }

    fn compute_proofs(
        &self,
        data: &[u8],
        _kzg_bundle: &Self::Bundle,
    ) -> Result<Vec<<Self::Bundle as crate::KzgBundle>::Proof>, Box<dyn std::error::Error>> {
        std::io::Error::new(std::io::ErrorKind::Other, "x").source();
        Ok(vec![MockProof {
            cks: crc32fast::hash(data),
        }])
    }

    fn verify_blob(
        &self,
        blob: &[u8],
        kzg_bundle: &Self::Bundle,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let proof = &self.compute_proofs(blob, kzg_bundle)?;
        Ok(proof == kzg_bundle.proof())
    }
}
