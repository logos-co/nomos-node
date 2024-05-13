use nomos_core::da::DaVerifier;

pub trait VerifierBackend: DaVerifier {
    type Settings;
    fn new(settings: Self::Settings) -> Self;
}
