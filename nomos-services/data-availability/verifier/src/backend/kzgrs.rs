// std
use core::fmt;
use std::collections::HashSet;
// crates
use blst::min_sig::SecretKey;
use kzgrs_backend::{common::blob::DaBlob, verifier::DaVerifier as NomosKzgrsVerifier};
use nomos_core::da::DaVerifier;
use serde::{Deserialize, Serialize};
// internal
use super::VerifierBackend;

#[derive(Debug)]
pub enum KzgrsDaVerifierError {
    VerificationError,
}

impl fmt::Display for KzgrsDaVerifierError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            KzgrsDaVerifierError::VerificationError => write!(f, "Verification failed"),
        }
    }
}

impl std::error::Error for KzgrsDaVerifierError {}

pub struct KzgrsDaVerifier {
    verifier: NomosKzgrsVerifier,
}

impl VerifierBackend for KzgrsDaVerifier {
    type Settings = KzgrsDaVerifierSettings;

    fn new(settings: Self::Settings) -> Self {
        let bytes = hex::decode(settings.sk).expect("Secret key string should decode to bytes");
        let secret_key =
            SecretKey::from_bytes(&bytes).expect("Secret key should be reconstructed from bytes");

        let verifier = NomosKzgrsVerifier::new(secret_key, settings.index);
        Self { verifier }
    }
}

impl DaVerifier for KzgrsDaVerifier {
    type DaBlob = DaBlob;
    type Error = KzgrsDaVerifierError;

    fn verify(&self, blob: &Self::DaBlob) -> Result<(), Self::Error> {
        let blob = blob.clone();
        // TODO: Prepare the domain depending the size, if fixed, so fixed domain, if not it needs
        // to come with some metadata.
        let domain_size = 2usize;
        self.verifier
            .verify(&blob, domain_size)
            .then_some(())
            .ok_or(KzgrsDaVerifierError::VerificationError)
    }
}

// TODO: `sk` and `nodes_public_keys` need to be fetched from the params provider service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KzgrsDaVerifierSettings {
    pub sk: String,
    pub index: HashSet<u32>,
}
