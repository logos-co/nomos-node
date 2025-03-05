use async_trait::async_trait;
use overwatch_rs::DynError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::api::http::{ApiRequest, ApiResponse};

pub mod http;

#[async_trait]
pub trait ApiBackend {
    type Settings: Clone;
    type Request: Send;
    type Response: Send;
    fn new(settings: Self::Settings) -> Self;
    async fn run(
        self,
    ) -> Result<(UnboundedSender<ApiRequest>, UnboundedReceiver<ApiResponse>), DynError>;
}
