use std::num::NonZeroUsize;

pub struct MockKzgSettings {
    pub rows: NonZeroUsize,
    pub cols: NonZeroUsize,
}

pub struct MockRowCommitment {
    data: Vec<u8>,
}

pub struct MockColCommitment {}

pub struct MockMasterCommitment {}

pub struct MockProof {}

pub struct MockKzgBundle {
    pub master_commitment: MockMasterCommitment,
    pub row_commitments: Vec<MockRowCommitment>,
    pub col_commitments: Vec<MockColCommitment>,
    pub proof: MockProof,
}

impl super::KzgBundle for MockKzgBundle {
    type RowCommitment = MockRowCommitment;

    type ColCommitment = MockColCommitment;

    type MasterCommitment = MockMasterCommitment;

    type Proof = MockProof;

    fn master_commitment(&self) -> &Self::MasterCommitment {
        &self.master_commitment
    }

    fn row_commitments(&self) -> &[Self::RowCommitment] {
        &self.row_commitments
    }

    fn col_commitments(&self) -> &[Self::ColCommitment] {
        &self.col_commitments
    }

    fn proof(&self) -> &Self::Proof {
        &self.proof
    }
}

pub struct MockKzg {
    pub settings: MockKzgSettings,
}

impl super::KzgProvider for MockKzg {
    type Bundle = MockKzgBundle;

    type Settings = MockKzgSettings;

    fn compute_commitment(
        &self,
        data: &[u8],
    ) -> Result<Self::Bundle, Box<dyn std::error::Error>> {
        let chunk_size = data.len() / self.settings.rows;
        let mut cur = 0;
        let mut row_commitments = Vec::with_capacity(self.settings.rows.get());
        for _ in 0..self.settings.rows.get() - 1 {
            let chunk = &data[cur..cur + chunk_size];
            cur += chunk_size;
            row_commitments.push(MockRowCommitment {
                data: chunk.to_vec(),
            });
        }
        let chunk = &data[cur..];
        row_commitments.push(MockRowCommitment {
            data: chunk.to_vec(),
        });

        todo!()
    }

    fn compute_proofs(
        &self,
        data: &[u8],
        kzg_bundle: &Self::Bundle,
    ) -> Result<Vec<<Self::Bundle as crate::KzgBundle>::Proof>, Box<dyn std::error::Error>> {
        todo!()
    }

    fn verify_blob(
        &self,
        blob: &[u8],
        kzg_bundle: &Self::Bundle,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        todo!()
    }
}
