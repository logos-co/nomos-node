mod dynamic_kzg;
mod types;

use crate::types::{Blob, Commitment, Proof};
use dynamic_kzg::{blob_to_kzg_commitment, compute_blob_kzg_proof, verify_blob_kzg_proof};
use kzg::types::kzg_settings::FsKZGSettings;
use kzg_traits::G1;
use std::error::Error;

// TODO: Can this be imported?
pub const FIELD_ELEMENTS_PER_BLOB: usize = 4096;
pub const BYTES_PER_BLOB: usize = BYTES_PER_FIELD_ELEMENT * FIELD_ELEMENTS_PER_BLOB;
pub const BYTES_PER_FIELD_ELEMENT: usize = 32;
pub const BYTES_PER_PROOF: usize = 48;
pub const BYTES_PER_COMMITMENT: usize = 48;

pub struct Commitment(pub(crate) FsG1);

pub struct Proof(pub(crate) FsG1);

impl Commitment {
    pub fn as_bytes_owned(&self) -> [u8; BYTES_PER_COMMITMENT] {
        self.0.to_bytes()
    }
}

impl Proof {
    pub fn as_bytes_owned(&self) -> [u8; BYTES_PER_PROOF] {
        self.0.to_bytes()
    }
}

pub fn compute_commitments(
    data: &[u8],
    settings: &FsKZGSettings,
) -> Result<Vec<Commitment>, Box<dyn Error>> {
    let blobs = data.chunks(BYTES_PER_BLOB).map(bytes_to_blob);
    let mut res = Vec::new();
    for blob in blobs {
        let mut blob = blob?;
        let blob_len = blob.len();
        // FIXME: is this actually ok?
        // fill the blob with zeros if it is not full
        if blob.len() != BYTES_PER_BLOB {
            let offset = BYTES_PER_BLOB - blob_len;
            blob.extend((0..offset).map(|_| FsFr::default()));
        }
        res.push(Commitment(blob_to_kzg_commitment_rust(&blob, settings)))
    }
    Ok(res)
}

pub fn compute_proofs(
    data: &[u8],
    commitments: &[Commitment],
    settings: &FsKZGSettings,
) -> Result<Vec<Proof>, Box<dyn Error>> {
    assert_eq!(data.len() / BYTES_PER_BLOB, commitments.len());
    let blobs = data.chunks(BYTES_PER_BLOB).map(bytes_to_blob);
    let mut res = Vec::new();
    for (blob, commitment) in blobs.zip(commitments) {
        let mut blob = blob?;
        let blob_len = blob.len();
        // FIXME: is this actually ok?
        // fill the blob with zeros if it is not full
        if blob.len() != BYTES_PER_BLOB {
            let offset = BYTES_PER_BLOB - blob_len;
            blob.extend((0..offset).map(|_| FsFr::default()));
        }
        res.push(Proof(compute_blob_kzg_proof_rust(
            &blob,
            &commitment.0,
            settings,
        )?))
    }
    Ok(res)
}

pub fn verify_blob(
    blob: &[u8],
    proof: &Proof,
    commitment: &Commitment,
    settings: &FsKZGSettings,
) -> Result<bool, Box<dyn Error>> {
    assert_eq!(blob.len(), BYTES_PER_BLOB);
    let blob = bytes_to_blob(blob)?;
    verify_blob_kzg_proof_rust(&blob, &proof.0, &commitment.0, settings).map_err(|e| e.into())
}
