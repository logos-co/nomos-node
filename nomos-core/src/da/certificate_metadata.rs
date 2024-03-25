use super::certificate::Certificate;

pub trait CertificateExtension {
    type Extension;
    fn extension(&self) -> Self::Extension;
}
