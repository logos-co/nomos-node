// std
use core::fmt;
// crates
use blst::{min_sig::PublicKey, min_sig::SecretKey};
use kzgrs_backend::{
    common::{attestation::Attestation, blob::DaBlob},
    verifier::DaVerifier as NomosKzgrsVerifier,
};
use nomos_core::da::DaVerifier;
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
        let verifier = NomosKzgrsVerifier::new(settings.sk, &settings.nodes_public_keys);
        Self { verifier }
    }
}

impl DaVerifier for KzgrsDaVerifier {
    type DaBlob = DaBlob;
    type Attestation = Attestation;
    type Error = KzgrsDaVerifierError;

    fn verify(&self, blob: &Self::DaBlob) -> Result<Self::Attestation, Self::Error> {
        let blob = blob.clone();
        match self.verifier.verify(blob) {
            Some(attestation) => Ok(attestation),
            None => Err(KzgrsDaVerifierError::VerificationError),
        }
    }
}

// TODO: `sk` and `nodes_public_keys` need to be fetched from the params provider service.
#[derive(Debug, Clone)]
pub struct KzgrsDaVerifierSettings {
    pub sk: SecretKey,
    pub nodes_public_keys: Vec<PublicKey>,
}
