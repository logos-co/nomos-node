use std::{net::SocketAddr, sync::Arc};

use async_trait::async_trait;
use common_http_client::CommonHttpClient;
use futures::stream::{FuturesUnordered, StreamExt};
use kzgrs_backend::common::blob::{DaBlob, DaBlobSharedCommitments};
use nomos_core::da::BlobId;
use overwatch_rs::DynError;
use rand::prelude::IteratorRandom;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tracing::error;

use crate::api::ApiBackend;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiBackendSettings {
    pub addresses: Vec<SocketAddr>,
}

/// Request types for the API backend.
#[derive(Debug)]
pub enum ApiRequest {
    GetCommitments { blob_id: BlobId },
}

/// Response types for the API backend.
#[derive(Debug)]
pub enum ApiResponse {
    Commitments {
        blob_id: BlobId,
        commitments: Option<DaBlobSharedCommitments>,
    },
    Error {
        blob_id: BlobId,
        error: DynError,
    },
}

pub struct HttApiBackend {
    clients: Vec<Arc<CommonHttpClient>>,
}

impl HttApiBackend {
    #[must_use]
    pub fn new(settings: &ApiBackendSettings) -> Self {
        let clients = settings
            .addresses
            .iter()
            .map(|addr| Arc::new(CommonHttpClient::new(&format!("http://{addr}"), None)))
            .collect();
        Self { clients }
    }

    async fn send_request(
        api_request: ApiRequest,
        client: Arc<CommonHttpClient>,
        reply_channel: UnboundedSender<ApiResponse>,
    ) -> Result<(), DynError> {
        let ApiRequest::GetCommitments { blob_id } = api_request;

        let result = client.get_commitments::<DaBlob>(blob_id).await;
        let reply = match result {
            Ok(commitments) => ApiResponse::Commitments {
                blob_id,
                commitments,
            },
            Err(e) => ApiResponse::Error {
                blob_id,
                error: e.into(),
            },
        };

        reply_channel
            .send(reply)
            .map_err(|e| DynError::from(Box::new(e)))
    }
}

#[async_trait]
impl ApiBackend for HttApiBackend {
    type Settings = ApiBackendSettings;
    type Request = ApiRequest;
    type Response = ApiResponse;

    fn new(settings: Self::Settings) -> Self {
        Self::new(&settings)
    }

    async fn run(
        self,
    ) -> Result<(UnboundedSender<ApiRequest>, UnboundedReceiver<ApiResponse>), DynError> {
        let (req_tx, mut req_rx) = unbounded_channel();
        let (res_tx, res_rx) = unbounded_channel();

        let clients = self.clients;

        tokio::spawn(async move {
            let mut buffered_requests = FuturesUnordered::new();
            loop {
                tokio::select! {
                     Some(request) = req_rx.recv() => {
                        let client = clients
                            .iter()
                            .choose(&mut rand::thread_rng())
                            .unwrap()
                            .clone();
                        buffered_requests.push(Self::send_request(request, client, res_tx.clone()));
                    },
                    Some(result) = buffered_requests.next(), if !buffered_requests.is_empty() => {
                        if let Err(e) = result {
                            error!("Request failed: {:?}", e);
                        }
                    }
                    else => break,
                }
            }
        });

        Ok((req_tx, res_rx))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rand::{thread_rng, Rng as _};
    use tokio::time::timeout;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, Request, ResponseTemplate,
    };

    use super::*;

    #[tokio::test]
    async fn test_request_success() {
        fn response_fn(_: &Request) -> ResponseTemplate {
            let delay_ms = thread_rng().gen_range(50..=100);
            ResponseTemplate::new(200)
                .set_body_json(DaBlobSharedCommitments::default())
                .set_delay(Duration::from_millis(delay_ms))
        }

        let mock_server = MockServer::start().await;

        let blob_id = [0; 32];
        let num_requests = 2;

        Mock::given(method("GET"))
            .and(path("/da/get-commitments"))
            .respond_with(response_fn)
            .expect(num_requests)
            .mount(&mock_server)
            .await;

        let settings = ApiBackendSettings {
            addresses: vec![*mock_server.address()],
        };
        let backend = HttApiBackend::new(&settings);
        let (req_tx, mut res_rx) = backend.run().await.expect("Failed to start backend");

        for _ in 0..num_requests {
            req_tx
                .send(ApiRequest::GetCommitments { blob_id })
                .expect("Failed to send request");
        }

        drop(req_tx);

        let responses = timeout(Duration::from_secs(5), async move {
            let mut collected = Vec::with_capacity(num_requests as usize);
            while let Some(response) = res_rx.recv().await {
                collected.push(response);
            }
            collected
        })
        .await
        .expect("Test timed out");

        for response in responses {
            assert!(matches!(
                response,
                ApiResponse::Commitments {
                    blob_id: _,
                    commitments: Some(_)
                }
            ));
        }

        mock_server.verify().await;
    }
}
