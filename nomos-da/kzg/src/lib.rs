mod dynamic_kzg;

#[cfg(feature = "mock")]
pub mod mock;
pub mod types;

pub mod kzg;

use std::error::Error;

pub const BYTES_PER_PROOF: usize = 48;
pub const BYTES_PER_COMMITMENT: usize = 48;

pub trait KzgBundle {
    type RowCommitment;
    type ColCommitment;
    type MasterCommitment;
    type Proof;
    type Nonce;

    fn master_commitment(&self) -> &Self::MasterCommitment;
    fn row_commitments(&self) -> &[Self::RowCommitment];
    fn col_commitments(&self) -> &[Self::ColCommitment];
    fn proof(&self) -> &[Self::Proof];
    fn nonce(&self) -> &Self::Nonce;
}

pub trait KzgProvider {
    type Bundle: KzgBundle<Nonce = Self::Nonce>;
    type Settings;
    type Nonce;

    fn compute_commitment(
        &self,
        data: &[u8],
        nonce: Self::Nonce,
    ) -> Result<Self::Bundle, Box<dyn Error>>;

    fn compute_proofs(
        &self,
        data: &[u8],
        kzg_bundle: &Self::Bundle,
    ) -> Result<Vec<<Self::Bundle as KzgBundle>::Proof>, Box<dyn Error>>;

    fn verify_blob(&self, blob: &[u8], kzg_bundle: &Self::Bundle) -> Result<bool, Box<dyn Error>>;
}
