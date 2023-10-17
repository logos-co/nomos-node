use overwatch_rs::overwatch::handle::OverwatchHandle;
use std::{net::SocketAddr, sync::Arc};

use axum::{Router, Server};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::Backend;

#[derive(Clone)]
pub struct AxumBackendSettings {
    pub da: OverwatchHandle,
    pub addr: SocketAddr,
}

pub struct AxumBackend {
    settings: Arc<AxumBackendSettings>,
}

#[derive(OpenApi)]
#[openapi(paths(), components(), tags())]
struct ApiDoc;

#[async_trait::async_trait]
impl Backend for AxumBackend {
    type Error = hyper::Error;
    type Settings = AxumBackendSettings;

    async fn new(settings: Self::Settings) -> Result<Self, Self::Error>
    where
        Self: Sized,
    {
        Ok(Self {
            settings: Arc::new(settings),
        })
    }

    async fn serve(self) -> Result<(), Self::Error> {
        let store = self.settings.clone();
        let app = Router::new()
            .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
            .with_state(store);

        Server::bind(&self.settings.addr)
            .serve(app.into_make_service())
            .await
    }
}
