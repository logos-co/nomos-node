// std
// crates
use reqwest::{Client, ClientBuilder, StatusCode, Url};
// internal
use nomos_executor::api::paths;

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
    pub async fn publish_blob(&self, blob: Vec<u8>) -> Result<(), Error> {
        let url = self
            .executor_address
            .join(paths::DA_ADD_BLOB)
            .expect("Url should build properly");
        let response = self
            .client
            .post(url)
            .body(blob)
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
}
