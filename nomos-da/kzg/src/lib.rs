mod dynamic_kzg;
mod kzg;
mod types;

use std::error::Error;

pub const BYTES_PER_PROOF: usize = 48;
pub const BYTES_PER_COMMITMENT: usize = 48;

pub trait KzgBundle {
    type RowCommitment;
    type ColCommitment;
    type MasterCommitment;
    type Proof;

    fn master_commitment(&self) -> Self::MasterCommitment;
    fn row_commitments(&self) -> Vec<Self::RowCommitment>;
    fn col_commitments(&self) -> Vec<Self::ColCommitment>;
}

pub trait KzgProvider {
    type Bundle: KzgBundle;
    type Settings;

    fn compute_commitment(
        data: &[u8],
        settings: &Self::Settings,
    ) -> Result<Self::Bundle, Box<dyn Error>>;

    fn compute_proofs(
        data: &[u8],
        kzg_bundle: &Self::Bundle,
        settings: &Self::Settings,
    ) -> Result<Vec<<Self::Bundle as KzgBundle>::Proof>, Box<dyn Error>>;

    fn verify_blob(
        blob: &[u8],
        kzg_bundle: &Self::Bundle,
        settings: &Self::Settings,
    ) -> Result<bool, Box<dyn Error>>;
}
