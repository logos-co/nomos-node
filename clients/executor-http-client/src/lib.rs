// std
// crates
use reqwest::{Client, ClientBuilder, StatusCode, Url};
use serde::Serialize;
// internal
use nomos_executor::api::{handlers::DispersalRequest, paths};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Internal server error: {0}")]
    Server(String),
    #[error(transparent)]
    Request(reqwest::Error),
}

#[derive(Clone)]
pub struct BasicAuthCredentials {
    username: String,
    password: Option<String>,
}

impl BasicAuthCredentials {
    pub fn new(username: String, password: Option<String>) -> Self {
        Self { username, password }
    }
}

#[derive(Clone)]
pub struct ExecutorHttpClient {
    client: Client,
    executor_address: Url,
    basic_auth: Option<BasicAuthCredentials>,
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
            basic_auth: None,
        }
    }
}

impl ExecutorHttpClient {
    pub fn new(
        client: Client,
        executor_address: Url,
        basic_auth: Option<BasicAuthCredentials>,
    ) -> Self {
        Self {
            client,
            executor_address,
            basic_auth,
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
            .join(paths::DISPERSE_DATA.trim_start_matches('/'))
            .expect("Url should build properly");

        let mut request = self.client.post(url).json(&req);
        if let Some(basic_auth) = &self.basic_auth {
            request = request.basic_auth(&basic_auth.username, basic_auth.password.as_deref());
        }

        let response = request.send().await.map_err(Error::Request)?;
        let status = response.status();
        let body = response.text().await.map_err(Error::Request)?;

        match status {
            StatusCode::OK => Ok(()),
            StatusCode::INTERNAL_SERVER_ERROR => Err(Error::Server(body)),
            _ => Err(Error::Server(format!(
                "Unexpected response [{}]: {}",
                status, body
            ))),
        }
    }
}
