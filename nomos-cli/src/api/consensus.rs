use super::CLIENT;
use cryptarchia_consensus::CryptarchiaInfo;
use nomos_core::header::HeaderId;
use reqwest::Url;

pub async fn cryptarchia_info(node: &Url) -> Result<CryptarchiaInfo, reqwest::Error> {
    const NODE_CRYPTARCHIA_INFO_PATH: &str = "cryptarchia/info";
    CLIENT
        .get(node.join(NODE_CRYPTARCHIA_INFO_PATH).unwrap())
        .send()
        .await?
        .json::<CryptarchiaInfo>()
        .await
}

pub async fn get_headers_info(
    node: &Url,
    from: Option<HeaderId>,
    to: Option<HeaderId>,
) -> Result<Vec<HeaderId>, reqwest::Error> {
    const NODE_CRYPTARCHIA_HEADERS_PATH: &str = "cryptarchia/headers";
    let mut req = CLIENT.get(node.join(NODE_CRYPTARCHIA_HEADERS_PATH).unwrap());
    if let Some(from) = from {
        req = req.query(&[("from", from)]);
    }
    if let Some(to) = to {
        req = req.query(&[("to", to)]);
    }

    req.send().await?.json().await
}
