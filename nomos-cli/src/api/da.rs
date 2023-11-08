use super::CLIENT;
use full_replication::Blob;
use nomos_core::da::blob;
use reqwest::Url;

pub async fn get_blobs(
    node: &Url,
    ids: Vec<<Blob as blob::Blob>::Hash>,
) -> Result<Vec<Blob>, reqwest::Error> {
    const BLOBS_PATH: &str = "da/blobs";
    CLIENT
        .post(node.join(BLOBS_PATH).unwrap())
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&ids).unwrap())
        .send()
        .await?
        .json()
        .await
}
