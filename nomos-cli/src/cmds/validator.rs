// std
use std::sync::mpsc::Sender;
use std::{error::Error, ops::Range};
// crates
use clap::Args;
use reqwest::{Client, Url};
use serde::{de::DeserializeOwned, Serialize};
// internal
use kzgrs_backend::{common::blob::DaBlob, dispersal::Index};
use nomos_core::da::blob::metadata;
use nomos_node::api::{handlers::GetRangeReq, paths};
use nomos_node::wire;

type RetrievalRes<Index> = Result<Vec<(Index, Vec<Vec<u8>>)>, Box<dyn Error + Send + Sync>>;

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
        let addr = self.addr;
        let from: Index = self.from.into();
        let to: Index = self.to.into();

        let (res_sender, res_receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || retrieve_data(res_sender, addr, app_id, from..to));

        match res_receiver.recv() {
            Ok(update) => match update {
                Ok(app_blobs) => {
                    for (index, blobs) in app_blobs.iter() {
                        tracing::info!("Index {:?} has {:} blobs", (index), blobs.len());
                        for blob in blobs.iter() {
                            let blob = wire::deserialize::<DaBlob>(blob).unwrap();
                            tracing::info!("Index {:?}; Blob: {blob:?}", index.to_u64());
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Error receiving data: {e}");
                    return Err(e);
                }
            },
            Err(e) => {
                tracing::error!("Failed to receive from client thread: {e}");
                return Err(Box::new(e));
            }
        }

        tracing::info!("Done");
        Ok(())
    }
}

#[tokio::main]
async fn retrieve_data(
    res_sender: Sender<RetrievalRes<Index>>,
    url: Url,
    app_id: [u8; 32],
    range: Range<Index>,
) {
    let res = get_app_data_range_from_node::<kzgrs_backend::dispersal::Metadata>(
        reqwest::Client::new(),
        url,
        app_id,
        range,
    )
    .await;
    res_sender.send(res).unwrap();
}

async fn get_app_data_range_from_node<Metadata>(
    client: Client,
    url: Url,
    app_id: Metadata::AppId,
    range: Range<Metadata::Index>,
) -> RetrievalRes<Metadata::Index>
where
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
        .json::<Vec<(Metadata::Index, Vec<Vec<u8>>)>>()
        .await
        .unwrap())
}
