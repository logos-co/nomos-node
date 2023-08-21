mod dynamic_kzg;
mod types;

use crate::types::{Blob, Commitment, Proof};
use dynamic_kzg::{blob_to_kzg_commitment, compute_blob_kzg_proof, verify_blob_kzg_proof};
use kzg::types::kzg_settings::FsKZGSettings;
use std::error::Error;

// TODO: Can this be imported?
pub const FIELD_ELEMENTS_PER_BLOB: usize = 4096;
pub const BYTES_PER_BLOB: usize = BYTES_PER_FIELD_ELEMENT * FIELD_ELEMENTS_PER_BLOB;
pub const BYTES_PER_FIELD_ELEMENT: usize = 32;
pub const BYTES_PER_PROOF: usize = 48;
pub const BYTES_PER_COMMITMENT: usize = 48;

pub fn compute_commitments(
    data: &[u8],
    settings: &FsKZGSettings,
) -> Result<Commitment, Box<dyn Error>> {
    let blob = Blob::from_bytes(data)?;
    Ok(Commitment(blob_to_kzg_commitment(
        &blob,
        settings,
        data.len(),
    )))
}

pub fn compute_proofs(
    data: &[u8],
    commitment: &Commitment,
    settings: &FsKZGSettings,
    chunk: usize,
) -> Result<Vec<Proof>, Box<dyn Error>> {
    let chunk_size = data.len() / chunk;
    let blobs = data.chunks(chunk_size).map(Blob::from_bytes);
    let mut res = Vec::new();
    for blob in blobs {
        let blob = blob?;
        res.push(Proof(compute_blob_kzg_proof(&blob, commitment, settings)?))
    }
    Ok(res)
}

pub fn verify_blob(
    blob: &[u8],
    proof: &Proof,
    commitment: &Commitment,
    settings: &FsKZGSettings,
) -> Result<bool, Box<dyn Error>> {
    let blob = Blob::from_bytes(blob)?;
    verify_blob_kzg_proof(&blob, commitment, proof, settings).map_err(|e| e.into())
}
