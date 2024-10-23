use std::path::PathBuf;

use clap::Args;
use executor_http_client::ExecutorHttpClient;
use kzgrs_backend::dispersal::Metadata;
use reqwest::Url;

#[derive(Args, Debug)]
pub struct Disseminate {
    /// Text to disseminate.
    #[clap(short, long, required_unless_present("file"))]
    pub data: Option<String>,
    /// File to disseminate.
    #[clap(short, long)]
    pub file: Option<PathBuf>,
    /// Application ID for dispersed data.
    #[clap(long)]
    pub app_id: String,
    /// Index for the Blob associated with Application ID.
    #[clap(long)]
    pub index: u64,
    /// Executor address which is responsible for dissemination.
    #[clap(long)]
    pub addr: Url,
}

impl Disseminate {
    pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::subscriber::set_global_default(tracing_subscriber::FmtSubscriber::new())
            .expect("setting tracing default failed");

        let client = ExecutorHttpClient::new(reqwest::Client::new(), self.addr.clone());

        let mut bytes: Vec<u8> = if let Some(data) = &self.data {
            data.clone().into_bytes()
        } else {
            let file_path = self.file.as_ref().unwrap();
            std::fs::read(file_path)?
        };

        let remainder = bytes.len() % 31;
        if remainder != 0 {
            bytes.resize(bytes.len() + (31 - remainder), 0);
        }

        let app_id: [u8; 32] = hex::decode(&self.app_id)?
            .try_into()
            .map_err(|_| "Invalid app_id")?;
        let metadata = Metadata::new(app_id, self.index.into());

        let (res_sender, res_receiver) = std::sync::mpsc::channel();
        tokio::spawn(async move {
            let res = client
                .publish_blob(bytes, metadata)
                .await
                .map_err(|err| format!("Failed to publish blob: {:?}", err));
            res_sender.send(res).unwrap();
        });

        while let Ok(update) = res_receiver.recv() {
            match update {
                Ok(_) => tracing::info!("Data successfully disseminated."),
                Err(e) => {
                    tracing::error!("Error disseminating data: {e}");
                    return Err(e.into());
                }
            }
        }

        tracing::info!("Done");
        Ok(())
    }
}
