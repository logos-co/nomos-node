use super::CLIENT;
use reqwest::{Error, Response, Url};
use serde::Serialize;

pub async fn send_blob_info<I>(node: &Url, info: &I) -> Result<Response, Error>
where
    I: Serialize,
{
    const NODE_CERT_PATH: &str = "mempool/add/blobinfo";
    CLIENT
        .post(node.join(NODE_CERT_PATH).unwrap())
        .json(info)
        .send()
        .await
}
