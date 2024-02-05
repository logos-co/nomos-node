use super::CLIENT;
use full_replication::Blob;
use nomos_core::da::blob;
use reqwest::Url;

pub async fn get_blobs(
    explorer: &Url,
    ids: Vec<<Blob as blob::Blob>::Hash>,
) -> Result<Vec<Blob>, reqwest::Error> {
    const BLOBS_PATH: &str = "da/blobs";
    CLIENT
        .post(explorer.join(BLOBS_PATH).unwrap())
        .json(&ids)
        .send()
        .await?
        .json()
        .await
}
