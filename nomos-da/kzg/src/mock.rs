pub struct MockSettings {}

pub struct MockRowCommitment {}

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

pub struct MockKzg {}

impl super::KzgProvider for MockKzg {
    type Bundle = MockKzgBundle;

    type Settings = MockSettings;

    fn compute_commitment(
        data: &[u8],
        settings: &Self::Settings,
    ) -> Result<Self::Bundle, Box<dyn std::error::Error>> {
        todo!()
    }

    fn compute_proofs(
        data: &[u8],
        kzg_bundle: &Self::Bundle,
        settings: &Self::Settings,
    ) -> Result<Vec<<Self::Bundle as crate::KzgBundle>::Proof>, Box<dyn std::error::Error>> {
        todo!()
    }

    fn verify_blob(
        blob: &[u8],
        kzg_bundle: &Self::Bundle,
        settings: &Self::Settings,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        todo!()
    }
}
