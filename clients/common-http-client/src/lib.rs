use std::{fmt::Debug, sync::Arc};

use nomos_core::da::blob::Blob;
use nomos_da_messages::http::da::{DABlobCommitmentsRequest, DaSamplingRequest};
use reqwest::{Client, ClientBuilder, RequestBuilder, StatusCode, Url};
use serde::{de::DeserializeOwned, Serialize};

// These could be moved into shared location, perhaps to upcoming `nomos-lib`
const DA_GET_SHARED_COMMITMENTS: &str = "/da/sampling/commitments";

const DA_GET_LIGHT_BLOB: &str = "/da/sampling/blob";

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Internal server error: {0}")]
    Server(String),
    #[error(transparent)]
    Request(#[from] reqwest::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
}

#[derive(Clone)]
pub struct BasicAuthCredentials {
    username: String,
    password: Option<String>,
}

impl BasicAuthCredentials {
    #[must_use]
    pub const fn new(username: String, password: Option<String>) -> Self {
        Self { username, password }
    }
}

#[derive(Clone)]
pub struct CommonHttpClient {
    client: Arc<Client>,
    basic_auth: Option<BasicAuthCredentials>,
}

impl CommonHttpClient {
    #[must_use]
    pub fn new(basic_auth: Option<BasicAuthCredentials>) -> Self {
        let client = ClientBuilder::new()
            .build()
            .expect("Client from default settings should be able to build");
        Self {
            client: Arc::new(client),
            basic_auth,
        }
    }
}

impl CommonHttpClient {
    pub async fn post<Req, Res>(&self, request_url: Url, request_body: &Req) -> Result<Res, Error>
    where
        Req: Serialize + ?Sized + Send + Sync,
        Res: DeserializeOwned + Send + Sync,
    {
        let request = self.client.post(request_url).json(request_body);
        self.execute_request::<Res>(request).await
    }

    pub async fn get<Req, Res>(&self, request_url: Url, request_body: &Req) -> Result<Res, Error>
    where
        Req: Serialize + ?Sized + Send + Sync,
        Res: DeserializeOwned + Send + Sync,
    {
        let request = self.client.get(request_url).json(request_body);
        self.execute_request::<Res>(request).await
    }

    async fn execute_request<Res: DeserializeOwned>(
        &self,
        mut request: RequestBuilder,
    ) -> Result<Res, Error> {
        if let Some(basic_auth) = &self.basic_auth {
            request = request.basic_auth(&basic_auth.username, basic_auth.password.as_deref());
        }

        let response = request.send().await.map_err(Error::Request)?;
        let status = response.status();
        let body = response.text().await.map_err(Error::Request)?;

        match status {
            StatusCode::OK => serde_json::from_str(&body)
                .map_err(|e| Error::Server(format!("Failed to parse response: {e}"))),
            StatusCode::INTERNAL_SERVER_ERROR => Err(Error::Server(body)),
            _ => Err(Error::Server(format!(
                "Unexpected response [{status}]: {body}",
            ))),
        }
    }

    /// Get the commitments for a Blob
    pub async fn get_commitments<B>(
        &self,
        base_url: Url,
        blob_id: B::BlobId,
    ) -> Result<Option<B::SharedCommitments>, Error>
    where
        B: Blob,
        B::SharedCommitments: DeserializeOwned + Send + Sync,
        <B as Blob>::BlobId: serde::Serialize + Send + Sync,
    {
        let request: DABlobCommitmentsRequest<B> = DABlobCommitmentsRequest { blob_id };
        let path = DA_GET_SHARED_COMMITMENTS.trim_start_matches('/');
        let request_url = base_url.join(path).map_err(Error::Url)?;
        self.get(request_url, &request).await
    }

    /// Get blob by blob id and column index
    pub async fn get_blob<B, C>(
        &self,
        base_url: Url,
        blob_id: B::BlobId,
        column_idx: B::ColumnIndex,
    ) -> Result<Option<C>, Error>
    where
        C: DeserializeOwned + Send + Sync,
        B: Blob + DeserializeOwned + Send + Sync,
        <B as Blob>::BlobId: serde::Serialize + Send + Sync,
        <B as Blob>::ColumnIndex: serde::Serialize + Send + Sync,
    {
        let request: DaSamplingRequest<B> = DaSamplingRequest {
            blob_id,
            share_idx: column_idx,
        };
        let path = DA_GET_LIGHT_BLOB.trim_start_matches('/');
        let request_url = base_url.join(path).map_err(Error::Url)?;
        self.get(request_url, &request).await
    }
}
