// std
use std::path::PathBuf;
// crates
use bytes::Bytes;
use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt},
};
// internal

// TODO: Rocksdb has a feature called BlobDB that handles largo blob storing, but further
// investigation needs to be done to see if rust wrapper supports it.
pub async fn load_blobs(base_dir: PathBuf, blob_id: &[u8]) -> Vec<Bytes> {
    let blob_id = hex::encode(blob_id);

    let mut path = base_dir;
    path.push(blob_id);

    let mut blobs = Vec::new();

    let mut column_files = match fs::read_dir(&path).await {
        Ok(entries) => entries,
        Err(e) => {
            tracing::error!("Failed to read directory: {}", e);
            return blobs;
        }
    };

    while let Some(entry) = column_files.next_entry().await.ok().flatten() {
        let mut file = match File::open(entry.path()).await {
            Ok(file) => file,
            Err(e) => {
                tracing::error!("Failed to open file: {}", e);
                continue;
            }
        };

        let mut contents = vec![];
        if let Err(e) = file.read_to_end(&mut contents).await {
            tracing::error!("Failed to read file: {}", e);
            continue;
        }

        blobs.push(Bytes::from(contents));
    }

    blobs
}

pub async fn load_blob(
    base_dir: PathBuf,
    blob_id: &[u8],
    column_idx: &[u8],
) -> Result<Bytes, std::io::Error> {
    let blob_id = hex::encode(blob_id);
    let column_file = hex::encode(column_idx);

    let mut path = base_dir;
    path.push(blob_id);
    path.push(column_file);

    let mut file = tokio::fs::File::open(path).await?;

    let mut data = Vec::new();
    file.read_to_end(&mut data).await?;

    Ok(Bytes::from(data))
}

pub async fn write_blob(
    base_dir: PathBuf,
    blob_id: &[u8],
    column_idx: &[u8],
    data: &[u8],
) -> Result<(), std::io::Error> {
    let blob_id = hex::encode(blob_id);
    let column_file = hex::encode(column_idx);

    let mut path = base_dir;
    path.push(blob_id);
    path.push(column_file);

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
