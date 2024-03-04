use super::CLIENT;
use carnot_engine::BlockId;
use full_replication::Certificate;
use nomos_core::block::Block;
use nomos_node::Tx;
use reqwest::Url;

pub async fn get_block_contents(
    explorer: &Url,
    block: &BlockId,
) -> Result<Option<Block<Tx, Certificate>>, reqwest::Error> {
    const BLOCK_PATH: &str = "explorer/blocks/depth";
    CLIENT
        .get(explorer.join(BLOCK_PATH).unwrap())
        .query(&[("from", block)])
        .send()
        .await?
        .json()
        .await
}
