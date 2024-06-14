use full_replication::{Certificate, CertificateVerificationParameters};

use crate::verify::MempoolVerificationProvider;

pub struct DaVerificationProvider {
    settings: CertificateVerificationParameters,
}

#[async_trait::async_trait]
impl MempoolVerificationProvider for DaVerificationProvider {
    type Payload = Certificate;
    type Parameters = CertificateVerificationParameters;
    type Settings = CertificateVerificationParameters;

    fn new(settings: Self::Settings) -> Self {
        Self { settings }
    }

    async fn get_parameters(&self, _: &Self::Payload) -> Self::Parameters {
        self.settings.clone()
    }
}
