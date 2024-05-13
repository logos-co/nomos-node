use super::metadata::Metadata;

pub trait VidCertificate: Metadata {
    type CertificateId;

    fn certificate_id(&self) -> Self::CertificateId;
    fn size(&self) -> usize;
}
