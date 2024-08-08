use crate::da::disseminate::{DisseminateApp, DisseminateAppServiceSettings, Settings, Status};
use clap::Args;
use nomos_log::{LoggerBackend, LoggerSettings};
use nomos_network::backends::libp2p::Libp2p as NetworkBackend;
use nomos_network::NetworkService;
use overwatch_rs::{overwatch::OverwatchRunner, services::ServiceData};
use reqwest::Url;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::Mutex;

#[derive(Args, Debug, Default)]
pub struct Disseminate {
    // TODO: accept bytes
    #[clap(short, long, required_unless_present("file"))]
    pub data: Option<String>,
    /// Path to the network config file
    #[clap(short, long)]
    pub network_config: PathBuf,
    /// The data availability protocol to use. Defaults to full replication.
    // #[clap(flatten)]
    // pub da_protocol: DaProtocolChoice,
    /// Timeout in seconds. Defaults to 120 seconds.
    #[clap(short, long, default_value = "120")]
    pub timeout: u64,
    /// Address of the node to send the certificate to
    /// for block inclusion, if present.
    #[clap(long)]
    pub node_addr: Option<Url>,
    /// File to write the certificate to, if present.
    #[clap(long)]
    pub output: Option<PathBuf>,
    /// File to disseminate
    #[clap(short, long)]
    pub file: Option<PathBuf>,
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

        let bytes: Box<[u8]> = if let Some(data) = &self.data {
            data.clone().as_bytes().into()
        } else {
            let file_path = self.file.as_ref().unwrap();
            let file_bytes = std::fs::read(file_path)?;
            file_bytes.into_boxed_slice()
        };

        let timeout = Duration::from_secs(self.timeout);
        let node_addr = self.node_addr.clone();
        let output = self.output.clone();
        let (payload_sender, payload_rx) = tokio::sync::mpsc::unbounded_channel();
        payload_sender.send(bytes).unwrap();
        std::thread::spawn(move || {
            OverwatchRunner::<DisseminateApp>::run(
                DisseminateAppServiceSettings {
                    network,
                    send_blob: Settings {
                        payload: Arc::new(Mutex::new(payload_rx)),
                        timeout,
                        status_updates,
                        node_addr,
                        output,
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
