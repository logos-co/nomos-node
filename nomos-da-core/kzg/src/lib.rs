mod dynamic_kzg;
mod types;

use crate::types::KzgSettings;
pub use crate::types::{Blob, Commitment, Proof};
pub use dynamic_kzg::{blob_to_kzg_commitment, compute_blob_kzg_proof, verify_blob_kzg_proof};
use std::error::Error;

pub const BYTES_PER_PROOF: usize = 48;
pub const BYTES_PER_COMMITMENT: usize = 48;

/// Compute a kzg commitment for the given data.
/// It works for arbitrary data, but the data must be a multiple of **32 bytes**.
/// The data is interpreted as a sequence of field elements. Each consisting of **32 bytes**.
pub fn compute_commitment(
    data: &[u8],
    settings: &KzgSettings,
) -> Result<Commitment, Box<dyn Error>> {
    let blob = Blob::from_bytes(data, settings)?;
    Ok(Commitment(blob_to_kzg_commitment(
        &blob,
        &settings.settings,
        data.len() / settings.bytes_per_field_element,
    )))
}

/// Compute a kzg proof for each field element in the given data.
/// It works for arbitrary data, but the data must be a multiple of **32 bytes**.
/// The data is interpreted as a sequence of field elements. Each consisting of **32 bytes**.
pub fn compute_proofs(
    data: &[u8],
    commitment: &Commitment,
    settings: &KzgSettings,
) -> Result<Vec<Proof>, Box<dyn Error>> {
    let blobs = data
        .chunks(settings.bytes_per_field_element)
        .map(|b| Blob::from_bytes(b, settings));
    let mut res = Vec::new();
    for blob in blobs {
        let blob = blob?;
        res.push(Proof(compute_blob_kzg_proof(&blob, commitment, settings)?))
    }
    Ok(res)
}

/// Verify a kzg proof for the given blob.
/// It works for arbitrary data, but the data must be a multiple of **32 bytes**.
/// The data is interpreted as a sequence of field elements. Each consisting of **32 bytes**.
pub fn verify_blob(
    blob: &[u8],
    proof: &Proof,
    commitment: &Commitment,
    settings: &KzgSettings,
) -> Result<bool, Box<dyn Error>> {
    let blob = Blob::from_bytes(blob, settings)?;
    verify_blob_kzg_proof(&blob, commitment, proof, settings).map_err(|e| e.into())
}

#[cfg(test)]
mod test {
    use super::*;
    use kzg::types::kzg_settings::FsKZGSettings;
    use kzg::utils::generate_trusted_setup;
    use kzg_traits::{FFTSettings, KZGSettings};

    #[test]
    fn test_compute_and_verify() -> Result<(), Box<dyn Error>> {
        let (g1s, g2s) = generate_trusted_setup(4096, [0; 32]);
        let fft_settings = kzg::types::fft_settings::FsFFTSettings::new(8).unwrap();
        let settings = FsKZGSettings::new(&g1s, &g2s, 4096, &fft_settings).unwrap();
        let kzg_settings = KzgSettings {
            settings,
            bytes_per_field_element: 32,
        };
        let blob = vec![0; 4096];
        let commitment = compute_commitment(&blob, &kzg_settings)?;
        let proofs = compute_proofs(&blob, &commitment, &kzg_settings)?;
        for proof in proofs {
            assert!(verify_blob(&blob, &proof, &commitment, &kzg_settings)?);
        }
        Ok(())
    }
}
