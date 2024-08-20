// std
use std::path::PathBuf;
// crates
use bytes::Bytes;
use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt},
};
// internal

// TODO: Rocksdb has a feature called BlobDB that handles largo blob storing, but further
// investigation needs to be done to see if rust wrapper supports it.
pub async fn load_blob(base_dir: PathBuf, blob_id: &[u8]) -> Option<Bytes> {
    let blob_id = hex::encode(blob_id);

    let mut path = base_dir;
    path.push(blob_id);

    let mut file = match File::open(path).await {
        Ok(file) => file,
        Err(e) => {
            tracing::error!("Failed to open file: {}", e);
            return None;
        }
    };

    let mut contents = vec![];
    if let Err(e) = file.read_to_end(&mut contents).await {
        tracing::error!("Failed to read file: {}", e);
        return None;
    }

    Some(Bytes::from(contents))
}

pub async fn write_blob(
    base_dir: PathBuf,
    blob_idx: &[u8],
    data: &[u8],
) -> Result<(), std::io::Error> {
    let blob_idx = hex::encode(blob_idx);

    let mut path = base_dir;
    path.push(blob_idx);

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        // In an unlikely scenario when a file already exists, rewrite the
        // contents.
        .truncate(true)
        .open(path)
        .await?;

    file.write_all(data).await
}
