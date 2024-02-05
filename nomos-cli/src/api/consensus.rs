use super::CLIENT;
use carnot_consensus::CarnotInfo;
use reqwest::Url;

pub async fn carnot_info(node: &Url) -> Result<CarnotInfo, reqwest::Error> {
    const NODE_CARNOT_INFO_PATH: &str = "carnot/info";
    CLIENT
        .get(node.join(NODE_CARNOT_INFO_PATH).unwrap())
        .send()
        .await?
        .json::<CarnotInfo>()
        .await
}
