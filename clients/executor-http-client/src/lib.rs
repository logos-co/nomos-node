// std
use std::ops::Range;
// crates
use reqwest::{Client, ClientBuilder, StatusCode, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
// internal
use nomos_core::da::blob::{
    self,
    metadata::{self, Metadata},
};
use nomos_executor::api::{handlers::DispersalRequest, paths};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Internal server error: {0}")]
    Server(String),
    #[error(transparent)]
    Request(reqwest::Error),
}

#[derive(Clone)]
pub struct ExecutorHttpClient {
    client: Client,
    executor_address: Url,
}

#[derive(Serialize, Deserialize)]
pub struct GetRangeReq<M: metadata::Metadata>
where
    <M as Metadata>::AppId: Serialize + DeserializeOwned,
    <M as Metadata>::Index: Serialize + DeserializeOwned,
{
    app_id: <M as Metadata>::AppId,
    range: Range<<M as Metadata>::Index>,
}

impl Default for ExecutorHttpClient {
    fn default() -> Self {
        let client = ClientBuilder::new()
            .build()
            .expect("Client from default settings should be able to build");
        let executor_address = Url::parse("https://127.0.0.1:3333").unwrap();
        Self {
            client,
            executor_address,
        }
    }
}

impl ExecutorHttpClient {
    pub fn new(client: Client, executor_address: Url) -> Self {
        Self {
            client,
            executor_address,
        }
    }

    /// Send a `Blob` to be dispersed
    pub async fn publish_blob<Metadata: Serialize>(
        &self,
        data: Vec<u8>,
        metadata: Metadata,
    ) -> Result<(), Error> {
        let req = DispersalRequest { data, metadata };
        let url = self
            .executor_address
            .join(paths::DISPERSE_DATA)
            .expect("Url should build properly");
        let response = self
            .client
            .post(url)
            .json(&req)
            .send()
            .await
            .map_err(Error::Request)?;
        match response.status() {
            StatusCode::OK => Ok(()),
            StatusCode::INTERNAL_SERVER_ERROR => Err(Error::Server(
                response.text().await.map_err(Error::Request)?,
            )),
            _ => unreachable!("As per the documentation it can only return 200 or 500 responses"),
        }
    }

    pub async fn get_app_data_range_from_node<Blob, Metadata>(
        &self,
        app_id: Metadata::AppId,
        range: Range<Metadata::Index>,
    ) -> Result<Vec<(Metadata::Index, Vec<Blob>)>, Error>
    where
        Blob: blob::Blob + DeserializeOwned,
        Metadata: metadata::Metadata + Serialize,
        <Metadata as metadata::Metadata>::Index: Serialize + DeserializeOwned,
        <Metadata as metadata::Metadata>::AppId: Serialize + DeserializeOwned,
    {
        let url = self
            .executor_address
            .join(paths::DA_GET_RANGE)
            .expect("Url should build properly");
        let req = &GetRangeReq::<Metadata> { app_id, range };

        Ok(self
            .client
            .post(url)
            .header("Content-Type", "application/json")
            .json(&req)
            .send()
            .await
            .unwrap()
            .json::<Vec<(Metadata::Index, Vec<Blob>)>>()
            .await
            .unwrap())
    }
}
