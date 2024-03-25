use crate::da::certificate_metadata::CertificateExtension;

pub trait VID: CertificateExtension {
    type CertificateId;

    fn certificate_id(&self) -> Self::CertificateId;
}
