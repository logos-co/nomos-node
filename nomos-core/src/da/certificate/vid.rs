use super::metadata::Metadata;

pub trait VID: Metadata {
    type CertificateId;

    fn certificate_id(&self) -> Self::CertificateId;
}
