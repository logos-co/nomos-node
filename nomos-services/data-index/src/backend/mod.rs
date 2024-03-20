use nomos_core::da::certificate::Certificate;
use overwatch_rs::DynError;

#[derive(Debug)]
pub enum DaIndexError {
    Dyn(DynError),
}

#[async_trait::async_trait]
pub trait DaIndexBackend {
    type Settings: Clone;

    type Certificate: Certificate;

    fn new(settings: Self::Settings) -> Self;

    async fn index_cert(&self, cert: Self::Certificate) -> Result<(), DaIndexError>;

    fn get_range(&self, meta: Vec<<Self::Certificate as Certificate>::Extension>);
}
