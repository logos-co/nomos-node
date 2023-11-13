use super::CLIENT;
use reqwest::{Error, Response, Url};
use serde::Serialize;

pub async fn send_certificate<C>(node: &Url, cert: &C) -> Result<Response, Error>
where
    C: Serialize,
{
    const NODE_CERT_PATH: &str = "mempool/add/cert";
    CLIENT
        .post(node.join(NODE_CERT_PATH).unwrap())
        .json(cert)
        .send()
        .await
}
