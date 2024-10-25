// std
use std::{error::Error, ops::Range};
// crates
use clap::Args;
use reqwest::{Client, Url};
use serde::{de::DeserializeOwned, Serialize};
// internal
use kzgrs_backend::{common::blob::DaBlob, dispersal::Index};
use nomos_core::da::blob::{self, metadata};
use nomos_node::api::{handlers::GetRangeReq, paths};

#[derive(Args, Debug)]
pub struct Retrieve {
    /// Application ID of data in Indexer.
    #[clap(long)]
    pub app_id: String,
    ///  Retrieve from this Index associated with Application ID.
    #[clap(long)]
    pub from: u64,
    /// Retrieve to this Index associated with Application ID.
    #[clap(long)]
    pub to: u64,
    /// Node address to retrieve appid blobs.
    #[clap(long)]
    pub addr: Url,
}

impl Retrieve {
    pub fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::subscriber::set_global_default(tracing_subscriber::FmtSubscriber::new())
            .expect("setting tracing default failed");

        let app_id: [u8; 32] = hex::decode(&self.app_id)?
            .try_into()
            .map_err(|_| "Invalid app_id")?;
        let from: Index = self.from.into();
        let to: Index = self.to.into();

        let (res_sender, res_receiver) = std::sync::mpsc::channel();
        tokio::spawn(async move {
            let res = get_app_data_range_from_node::<DaBlob, kzgrs_backend::dispersal::Metadata>(
                reqwest::Client::new(),
                self.addr.clone(),
                app_id,
                from..to,
            )
            .await
            .map_err(|err| format!("Failed to retrieve data form appid {app_id:?}: {err:?}",));
            res_sender.send(res).unwrap();
        });

        match res_receiver.recv() {
            Ok(update) => match update {
                Ok(app_blobs) => {
                    for (index, blobs) in app_blobs.iter() {
                        tracing::info!("Index {:?} has {:} blobs", (index), blobs.len());
                        for blob in blobs.iter() {
                            tracing::info!("Index {:?}; Blob: {blob:?}", index.to_u64());
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Error receiving data: {e}");
                    return Err(e.into());
                }
            },
            Err(e) => {
                tracing::error!("Failed to receive from client thread: {e}");
                return Err(e.into());
            }
        }

        tracing::info!("Done");
        Ok(())
    }
}

pub async fn get_app_data_range_from_node<Blob, Metadata>(
    client: Client,
    url: Url,
    app_id: Metadata::AppId,
    range: Range<Metadata::Index>,
) -> Result<Vec<(Metadata::Index, Vec<Blob>)>, Box<dyn Error>>
where
    Blob: blob::Blob + DeserializeOwned,
    Metadata: metadata::Metadata + Serialize,
    <Metadata as metadata::Metadata>::Index: Serialize + DeserializeOwned,
    <Metadata as metadata::Metadata>::AppId: Serialize + DeserializeOwned,
{
    let url = url
        .join(paths::DA_GET_RANGE)
        .expect("Url should build properly");
    let req = &GetRangeReq::<Metadata> { app_id, range };

    Ok(client
        .post(url)
        .header("Content-Type", "application/json")
        .json(&req)
        .send()
        .await
        .unwrap()
        .json::<Vec<(Metadata::Index, Vec<Blob>)>>()
        .await
        .unwrap())
}
