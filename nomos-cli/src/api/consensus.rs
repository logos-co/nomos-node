use super::CLIENT;
use consensus_engine::{Block, BlockId};
use nomos_consensus::CarnotInfo;
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

pub async fn get_blocks_info(
    node: &Url,
    from: Option<BlockId>,
    to: Option<BlockId>,
) -> Result<Vec<Block>, reqwest::Error> {
    const NODE_CARNOT_INFO_PATH: &str = "carnot/blocks";
    let mut req = CLIENT.get(node.join(NODE_CARNOT_INFO_PATH).unwrap());
    if let Some(from) = from {
        req = req.query(&[("from", from)]);
    }
    if let Some(to) = to {
        req = req.query(&[("to", to)]);
    }

    req.send().await?.json().await
}
