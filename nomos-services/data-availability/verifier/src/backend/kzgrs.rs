use core::fmt;

use kzgrs_backend::{
    common::blob::DaBlob, global::global_parameters_from_file,
    verifier::DaVerifier as NomosKzgrsVerifier,
};
use nomos_core::da::{blob::Blob, DaVerifier};
use serde::{Deserialize, Serialize};

use super::VerifierBackend;

#[derive(Debug)]
pub enum KzgrsDaVerifierError {
    VerificationError,
}

impl fmt::Display for KzgrsDaVerifierError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::VerificationError => write!(f, "Verification failed"),
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
        let global_params = global_parameters_from_file(&settings.global_params_path)
            .expect("Global parameters has to be loaded from file");

        let verifier = NomosKzgrsVerifier::new(global_params);
        Self { verifier }
    }
}

impl DaVerifier for KzgrsDaVerifier {
    type DaBlob = DaBlob;
    type Error = KzgrsDaVerifierError;

    fn verify(
        &self,
        commitments: &<Self::DaBlob as Blob>::SharedCommitments,
        light_blob: &<Self::DaBlob as Blob>::LightBlob,
    ) -> Result<(), Self::Error> {
        // TODO: Prepare the domain depending the size, if fixed, so fixed domain, if
        // not it needs to come with some metadata.
        let domain_size = 2usize;
        self.verifier
            .verify(commitments, light_blob, domain_size)
            .then_some(())
            .ok_or(KzgrsDaVerifierError::VerificationError)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KzgrsDaVerifierSettings {
    pub global_params_path: String,
}
