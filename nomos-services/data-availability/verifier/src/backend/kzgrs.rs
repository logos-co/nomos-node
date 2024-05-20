use blst::{min_sig::PublicKey, min_sig::SecretKey};
// std
// crates
use kzgrs_backend::{
    common::Attestation,
    verifier::{DaBlob, DaVerifier as NomosKzgrsVerifier},
};
use nomos_core::da::DaVerifier;
use overwatch_rs::DynError;

use super::VerifierBackend;
// internal

pub struct KzgrsDaVerifier {
    verifier: NomosKzgrsVerifier,
}

impl VerifierBackend for KzgrsDaVerifier {
    type Settings = KzgrsDaVerifierSettings;

    fn new(settings: Self::Settings) -> Self {
        let verifier = NomosKzgrsVerifier::new(settings.sk, &settings.nodes_public_keys);
        Self { verifier }
    }
}

impl DaVerifier for KzgrsDaVerifier {
    type DaBlob = DaBlob;
    type Attestation = Attestation;
    type Error = DynError;

    fn verify(&self, blob: &Self::DaBlob) -> Result<Self::Attestation, DynError> {
        let blob = blob.clone();
        match self.verifier.verify(blob) {
            Some(attestation) => Ok(attestation),
            None => Err("Failed to attest the blob".into()),
        }
    }
}

// TODO: `sk` and `nodes_public_keys` need to be fetched from the params provider service.
pub struct KzgrsDaVerifierSettings {
    pub sk: SecretKey,
    pub nodes_public_keys: Vec<PublicKey>,
}
