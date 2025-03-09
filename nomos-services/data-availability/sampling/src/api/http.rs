use async_trait::async_trait;
use common_http_client::CommonHttpClient;
use kzgrs_backend::common::blob::{DaBlob, DaBlobSharedCommitments};
use nomos_core::da::BlobId;
use overwatch_rs::DynError;
use rand::prelude::IteratorRandom;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tracing::error;
use url::Url;

use crate::api::ApiAdapter;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiAdapterSettings {
    pub addresses: Vec<Url>,
}

pub struct HttApiAdapter {
    clients: Vec<CommonHttpClient>,
}

#[async_trait]
impl ApiAdapter for HttApiAdapter {
    type Settings = ApiAdapterSettings;
    type Blob = DaBlob;
    type BlobId = BlobId;
    type Commitments = DaBlobSharedCommitments;
    fn new(settings: Self::Settings) -> Self {
        let clients = settings
            .addresses
            .iter()
            .map(|addr| CommonHttpClient::new(addr.to_owned(), None))
            .collect();
        Self { clients }
    }

    async fn request_commitments(
        &self,
        blob_id: Self::BlobId,
        reply_channel: oneshot::Sender<Option<Self::Commitments>>,
    ) -> Result<(), DynError> {
        let Some(client) = self.clients.iter().choose(&mut rand::thread_rng()) else {
            error!("No clients available");
            if reply_channel.send(None).is_err() {
                error!("Failed to send commitments reply");
            }
            return Ok(());
        };
        match client.get_commitments::<Self::Blob>(blob_id).await {
            Ok(commitments) => {
                if reply_channel.send(commitments).is_err() {
                    error!("Failed to send commitments reply");
                }
            }
            Err(e) => {
                error!("Failed to get commitments: {}", e);
            }
        }

        Ok(())
    }
}
