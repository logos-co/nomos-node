// std
use std::{path::PathBuf, sync::Arc, time::Duration};
// crates
use clap::Args;
use kzgrs_backend::dispersal::Metadata;
use nomos_da_network_service::NetworkService;
use nomos_log::{LoggerBackend, LoggerSettings};
use overwatch_rs::{overwatch::OverwatchRunner, services::ServiceData};
use reqwest::Url;
use tokio::sync::Mutex;
// internal
use crate::da::{
    disseminate::{DisseminateApp, DisseminateAppServiceSettings, KzgrsSettings, Settings, Status},
    NetworkBackend,
};

#[derive(Args, Debug, Default)]
pub struct Disseminate {
    // TODO: accept bytes
    #[clap(short, long, required_unless_present("file"))]
    pub data: Option<String>,
    /// Path to the network config file
    #[clap(short, long)]
    pub network_config: PathBuf,
    /// Timeout in seconds. Defaults to 120 seconds.
    #[clap(short, long, default_value = "120")]
    pub timeout: u64,
    /// Address of the node to send the certificate to
    /// for block inclusion, if present.
    #[clap(long)]
    pub node_addr: Option<Url>,
    // Application ID for dispersed data.
    #[clap(long)]
    pub app_id: String,
    // Index for the Blob associated with Application ID.
    #[clap(long)]
    pub index: u64,
    // Use Kzg RS cache.
    #[clap(long)]
    pub with_cache: bool,
    // Number of columns to use for encoding.
    #[clap(long, default_value = "4096")]
    pub columns: usize,
    // Duration in seconds to wait before publishing blob info.
    #[clap(short, long, default_value = "5")]
    pub wait_until_disseminated: u64,
    /// File to write the certificate to, if present.
    #[clap(long)]
    pub output: Option<PathBuf>,
    /// File to disseminate
    #[clap(short, long)]
    pub file: Option<PathBuf>,
    // Path to the KzgRs global parameters.
    #[clap(long)]
    pub global_params_path: String,
}

impl Disseminate {
    pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::subscriber::set_global_default(tracing_subscriber::FmtSubscriber::new())
            .expect("setting tracing default failed");
        let network = serde_yaml::from_reader::<
            _,
            <NetworkService<NetworkBackend> as ServiceData>::Settings,
        >(std::fs::File::open(&self.network_config)?)?;
        let (status_updates, rx) = std::sync::mpsc::channel();

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
        let timeout = Duration::from_secs(self.timeout);
        let node_addr = self.node_addr.clone();
        let output = self.output.clone();
        let num_columns = self.columns;
        let with_cache = self.with_cache;
        let wait_until_disseminated = Duration::from_secs(self.wait_until_disseminated);
        let global_params_path = self.global_params_path.clone();
        let (payload_sender, payload_rx) = tokio::sync::mpsc::unbounded_channel();
        payload_sender.send(bytes.into_boxed_slice()).unwrap();

        std::thread::spawn(move || {
            OverwatchRunner::<DisseminateApp>::run(
                DisseminateAppServiceSettings {
                    network,
                    send_blob: Settings {
                        payload: Arc::new(Mutex::new(payload_rx)),
                        timeout,
                        kzgrs_settings: KzgrsSettings {
                            num_columns,
                            with_cache,
                        },
                        metadata,
                        status_updates,
                        node_addr,
                        wait_until_disseminated,
                        output,
                        global_params_path,
                    },
                    logger: LoggerSettings {
                        backend: LoggerBackend::None,
                        ..Default::default()
                    },
                },
                None,
            )
            .unwrap()
            .wait_finished();
        });
        // drop to signal we're not going to send more blobs
        drop(payload_sender);
        tracing::info!("{}", rx.recv().unwrap());
        while let Ok(update) = rx.recv() {
            if let Status::Err(e) = update {
                tracing::error!("{e}");
                return Err(e);
            }
            tracing::info!("{}", update);
        }
        tracing::info!("done");
        Ok(())
    }
}
