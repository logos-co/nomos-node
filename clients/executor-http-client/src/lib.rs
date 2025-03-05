pub use common_http_client::BasicAuthCredentials;
use common_http_client::{CommonHttpClient, Error};
use nomos_core::da::blob::Blob;
use nomos_executor::api::{handlers::DispersalRequest, paths};
use serde::{de::DeserializeOwned, Serialize};

#[derive(Clone)]
pub struct ExecutorHttpClient {
    client: CommonHttpClient,
}

impl Default for ExecutorHttpClient {
    fn default() -> Self {
        let executor_address = "https://127.0.0.1:3333";
        Self {
            client: CommonHttpClient::new(executor_address, None),
        }
    }
}

impl ExecutorHttpClient {
    #[must_use]
    pub fn new(base_address: &str, basic_auth: Option<BasicAuthCredentials>) -> Self {
        Self {
            client: CommonHttpClient::new(base_address, basic_auth),
        }
    }

    /// Send a `Blob` to be dispersed
    pub async fn publish_blob<Metadata>(
        &self,
        data: Vec<u8>,
        metadata: Metadata,
    ) -> Result<(), Error>
    where
        Metadata: Serialize + Send + Sync,
    {
        let req = DispersalRequest { data, metadata };
        let path = paths::DISPERSE_DATA.trim_start_matches('/');

        self.client
            .post::<DispersalRequest<Metadata>, ()>(path, &req)
            .await
    }

    /// Get the commitments for a specific `BlobId`
    pub async fn get_commitments<B>(
        &self,
        blob_id: B::BlobId,
    ) -> Result<Option<B::SharedCommitments>, Error>
    where
        B: Blob + Send,
        <B as Blob>::BlobId: Serialize + Send + Sync,
        <B as Blob>::SharedCommitments: DeserializeOwned + Send + Sync,
    {
        self.client.get_commitments::<B>(blob_id).await
    }

    /// Get blob by blob id and column index
    pub async fn get_blob<B, C>(
        &self,
        blob_id: B::BlobId,
        column_idx: B::ColumnIndex,
    ) -> Result<Option<C>, Error>
    where
        C: DeserializeOwned + Send + Sync,
        B: Blob + DeserializeOwned + Send + Sync,
        <B as Blob>::BlobId: serde::Serialize + Send + Sync,
        <B as Blob>::ColumnIndex: serde::Serialize + Send + Sync,
    {
        self.client.get_blob::<B, C>(blob_id, column_idx).await
    }
}
